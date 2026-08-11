package io.github.mith_mmk.wml2viewer.data.source

import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.OutputStream

class SourceTransferEngineTest {
    @Test
    fun recoveredBackupNameFallsBackToABoundedProviderSafeName() {
        val recovered = recoveredBackupName("a".repeat(250) + ".archive-extension", "job-123456789")

        assertThat(recovered).isEqualTo("recovered-job12345.archive-extensi")
        assertThat(recovered.toByteArray(Charsets.UTF_8).size).isAtMost(120)

        val multibyte = recoveredBackupName("長".repeat(80) + ".cbz", "job-123456789")
        assertThat(multibyte).isEqualTo("recovered-job12345.cbz")
        assertThat(multibyte.toByteArray(Charsets.UTF_8).size).isAtMost(120)
    }

    @Test
    fun crossProviderMoveVerifiesThenCommitsBeforeDeletingSource() = runTest {
        val source = MemoryProvider("source").apply { addFile("book.cbz", "pages".toByteArray()) }
        val destination = MemoryProvider("destination")
        val engine = engine(source, destination)
        val events = mutableListOf<String>()
        val result = engine.transfer(
            source.file("book.cbz"), destination.root, "copy.cbz", CollisionPolicy.FAIL, TransferOperation.MOVE,
            journal = object : TransferJournal {
                override suspend fun copying(
                    temporary: EntryRef,
                    plannedFinalName: String,
                    replacementBackupName: String?,
                ) {
                    events += "copying:$plannedFinalName"
                }
                override suspend fun verifying(verification: WriteVerification) { events += "verifying" }
                override suspend fun committed(destination: EntryRef) { events += "committed" }
                override suspend fun deletingSource() { events += "deleting" }
            },
        )
        assertThat(events).containsExactly("copying:copy.cbz", "verifying", "committed", "deleting").inOrder()
        assertThat(result.sha256).hasLength(64)
        assertThat(source.exists("book.cbz")).isFalse()
        assertThat(destination.bytes("copy.cbz")).isEqualTo("pages".toByteArray())
    }

    @Test
    fun cancellationKeepsSource() {
        val source = MemoryProvider("source").apply {
            addFile("book.cbz", "pages".toByteArray())
        }
        val destination = MemoryProvider("destination")
        val engine = engine(source, destination)
        assertThrows(SourceException::class.java) {
            runTest {
                engine.transfer(source.file("book.cbz"), destination.root, "copy.cbz", CollisionPolicy.FAIL, TransferOperation.MOVE, isCancelled = { true })
            }
        }
        assertThat(source.exists("book.cbz")).isTrue()
        assertThat(destination.exists("copy.cbz")).isFalse()
    }

    @Test
    fun coroutineInterruptionIsNotConvertedToExplicitUserCancellation() = runTest {
        val source = MemoryProvider("source").apply { addFile("book.cbz", "pages".toByteArray()) }
        val destination = MemoryProvider("destination")

        val error = runCatching {
            engine(source, destination).transfer(
                source.file("book.cbz"),
                destination.root,
                "copy.cbz",
                CollisionPolicy.FAIL,
                TransferOperation.MOVE,
                isCancelled = { throw CancellationException("worker stopped") },
            )
        }.exceptionOrNull()

        assertThat(error).isInstanceOf(CancellationException::class.java)
        assertThat(error).isNotInstanceOf(SourceException::class.java)
        assertThat(source.exists("book.cbz")).isTrue()
        assertThat(destination.exists("copy.cbz")).isFalse()
    }

    @Test
    fun sameProviderDirectoryMoveRejectsUnjournaledNativeOnlyCapability() = runTest {
        val provider = MemoryProvider("memory").apply { addDirectory("series") }
        val error = runCatching {
            engine(provider).transfer(
                provider.directory("series"), provider.root, "renamed",
                CollisionPolicy.FAIL, TransferOperation.MOVE,
            )
        }.exceptionOrNull()
        assertThat(error).isInstanceOf(SourceException::class.java)
        assertThat((error as SourceException).code).isEqualTo(SourceErrorCode.UNSUPPORTED)
        assertThat(provider.nativeMoveCalls).isEqualTo(0)
        assertThat(provider.exists("series")).isTrue()
    }

    @Test
    fun samePathMoveReplaceIsNoOpForPathBasedReference() = runTest {
        val provider = MemoryProvider("smb:path").apply {
            addFile("book.cbz", "original".toByteArray())
        }

        val result = engine(provider).transfer(
            provider.file("book.cbz"),
            provider.root,
            "book.cbz",
            CollisionPolicy.REPLACE,
            TransferOperation.MOVE,
        )

        assertThat(result.skipped).isTrue()
        assertThat(result.sourceDeleted).isFalse()
        assertThat(provider.bytes("book.cbz")).isEqualTo("original".toByteArray())
        assertThat(provider.deleteCalls).isEqualTo(0)
    }

    @Test
    fun samePathCopyStillUsesCollisionPolicy() = runTest {
        val provider = MemoryProvider("smb:path").apply {
            addFile("book.cbz", "original".toByteArray())
        }
        val engine = engine(provider)

        val failure = runCatching {
            engine.transfer(
                provider.file("book.cbz"), provider.root, "book.cbz",
                CollisionPolicy.FAIL, TransferOperation.COPY,
            )
        }.exceptionOrNull()
        assertThat(failure).isInstanceOf(SourceException::class.java)
        assertThat((failure as SourceException).code).isEqualTo(SourceErrorCode.ALREADY_EXISTS)

        val skipped = engine.transfer(
            provider.file("book.cbz"), provider.root, "book.cbz",
            CollisionPolicy.SKIP, TransferOperation.COPY,
        )
        assertThat(skipped.skipped).isTrue()

        val replaced = engine.transfer(
            provider.file("book.cbz"), provider.root, "book.cbz",
            CollisionPolicy.REPLACE, TransferOperation.COPY,
        )
        assertThat(replaced.skipped).isTrue()
        assertThat(provider.bytes("book.cbz")).isEqualTo("original".toByteArray())

        val kept = engine.transfer(
            provider.file("book.cbz"), provider.root, "book.cbz",
            CollisionPolicy.KEEP_BOTH, TransferOperation.COPY,
        )
        assertThat(kept.destination).isEqualTo(provider.file("book (2).cbz"))
        assertThat(provider.bytes("book.cbz")).isEqualTo("original".toByteArray())
        assertThat(provider.bytes("book (2).cbz")).isEqualTo("original".toByteArray())
    }

    @Test
    fun crossProviderMoveRejectsUndeletableSourceBeforeCreatingDestination() = runTest {
        val source = MemoryProvider("source", canMoveFiles = false, canDeleteFiles = false).apply {
            addFile("book.cbz", "original".toByteArray())
        }
        val destination = MemoryProvider("destination")

        val error = runCatching {
            engine(source, destination).transfer(
                source.file("book.cbz"), destination.root, "book.cbz",
                CollisionPolicy.FAIL, TransferOperation.MOVE,
            )
        }.exceptionOrNull()

        assertThat(error).isInstanceOf(SourceException::class.java)
        assertThat((error as SourceException).code).isEqualTo(SourceErrorCode.UNSUPPORTED)
        assertThat(source.exists("book.cbz")).isTrue()
        assertThat(destination.createCalls).isEqualTo(0)
    }

    @Test
    fun sameProviderFileMoveRequiresDeleteEvenWhenNativeMoveExists() = runTest {
        val provider = MemoryProvider("source", canMoveFiles = true, canDeleteFiles = false).apply {
            addFile("book.cbz", "original".toByteArray())
        }

        val error = runCatching {
            engine(provider).transfer(
                provider.file("book.cbz"), provider.root, "renamed.cbz",
                CollisionPolicy.FAIL, TransferOperation.MOVE,
            )
        }.exceptionOrNull()

        assertThat(error).isInstanceOf(SourceException::class.java)
        assertThat((error as SourceException).code).isEqualTo(SourceErrorCode.UNSUPPORTED)
        assertThat(provider.nativeMoveCalls).isEqualTo(0)
        assertThat(provider.exists("book.cbz")).isTrue()
    }

    private fun engine(vararg providers: SourceProvider) = SourceTransferEngine(SourceProviderRegistry(providers.asList()), 4 * 1024)
}

private class MemoryProvider(
    override val providerId: String,
    private val canMoveFiles: Boolean = true,
    private val canDeleteFiles: Boolean = true,
) : SourceProvider {
    private data class Node(val name: String, val kind: EntryKind, val bytes: ByteArray?)
    private val nodes = linkedMapOf<String, Node>()
    override val root = EntryRef(providerId, "root")
    override val capabilities = SourceCapabilities(
        canList = true,
        canRead = true,
        canCreate = true,
        canCopyWithinProvider = true,
        canMoveWithinProvider = canMoveFiles,
        canDelete = canDeleteFiles,
    )
    var nativeMoveCalls = 0
    var deleteCalls = 0
    var createCalls = 0

    fun addFile(name: String, bytes: ByteArray) { nodes[name] = Node(name, EntryKind.FILE, bytes) }
    fun addDirectory(name: String) { nodes[name] = Node(name, EntryKind.DIRECTORY, null) }
    fun file(name: String) = EntryRef(providerId, name)
    fun directory(name: String) = EntryRef(providerId, name)
    fun exists(name: String) = name in nodes
    fun bytes(name: String) = nodes.getValue(name).bytes

    override suspend fun list(parent: EntryRef) = nodes.values.map(::entry)
    override suspend fun stat(entry: EntryRef) = if (entry == root) {
        SourceEntry(root, null, "root", EntryKind.DIRECTORY, null, null, null, effectiveCapabilities = capabilities)
    } else entry(nodes[entry.opaqueId] ?: throw SourceException(SourceErrorCode.NOT_FOUND, "missing"))
    override suspend fun openRead(entry: EntryRef): SourceRead {
        val node = nodes[entry.opaqueId] ?: throw SourceException(SourceErrorCode.NOT_FOUND, "missing")
        return SourceRead(ByteArrayInputStream(node.bytes!!), node.bytes.size.toLong(), "application/octet-stream")
    }
    override suspend fun create(parent: EntryRef, request: CreateRequest): AtomicWriteSession {
        createCalls++
        val existing = nodes.keys.toSet()
        return when (val resolution = CollisionResolver.resolve(request.name, request.collisionPolicy) { it in existing }) {
            CollisionResolution.Skip -> MemorySkippedWriteSession(file(request.name))
            is CollisionResolution.Use -> memoryWriteSession(resolution.name)
        }
    }

    private fun memoryWriteSession(finalName: String): AtomicWriteSession = object : AtomicWriteSession {
        private val buffer = ByteArrayOutputStream()
        override val temporaryRef = EntryRef(providerId, ".temp-$finalName")
        override val plannedFinalName = finalName
        override val replacementBackupName: String? = null
        override val output: OutputStream = buffer
        private var verified = false
        override suspend fun verify(expected: WriteVerification): Boolean =
            buffer.toByteArray().inputStream().sha256AndSize().let { (it == expected).also { ok -> verified = ok } }
        override suspend fun commit(): EntryRef {
            check(verified)
            addFile(finalName, buffer.toByteArray())
            return file(finalName)
        }
        override suspend fun abort() = Unit
    }
    override suspend fun createDirectory(parent: EntryRef, name: String, collisionPolicy: CollisionPolicy): EntryRef {
        addDirectory(name)
        return directory(name)
    }
    override suspend fun copy(source: EntryRef, destinationParent: EntryRef, destinationName: String, collisionPolicy: CollisionPolicy): EntryRef {
        val node = nodes.getValue(source.opaqueId)
        nodes[destinationName] = node.copy(name = destinationName, bytes = node.bytes?.copyOf())
        return EntryRef(providerId, destinationName)
    }
    override suspend fun move(source: EntryRef, destinationParent: EntryRef, destinationName: String, collisionPolicy: CollisionPolicy): EntryRef {
        nativeMoveCalls++
        val node = nodes.remove(source.opaqueId) ?: throw SourceException(SourceErrorCode.NOT_FOUND, "missing")
        nodes[destinationName] = node.copy(name = destinationName)
        return EntryRef(providerId, destinationName)
    }
    override suspend fun rename(entry: EntryRef, newName: String, collisionPolicy: CollisionPolicy) = move(entry, root, newName, collisionPolicy)
    override suspend fun trashOrDelete(entry: EntryRef, allowPermanentDelete: Boolean): DeleteDisposition {
        deleteCalls++
        if (nodes.remove(entry.opaqueId) == null) throw SourceException(SourceErrorCode.NOT_FOUND, "missing")
        return DeleteDisposition.PERMANENTLY_DELETED
    }
    override suspend fun thumbnail(entry: EntryRef, maxWidth: Int, maxHeight: Int) = null
    private fun entry(node: Node) = SourceEntry(
        EntryRef(providerId, node.name), root, node.name, node.kind, null, node.bytes?.size?.toLong(), null,
        effectiveCapabilities = SourceCapabilities(
            canList = node.kind == EntryKind.DIRECTORY,
            canRead = node.kind == EntryKind.FILE,
            canCreate = node.kind == EntryKind.DIRECTORY,
            canCopyWithinProvider = true,
            canMoveWithinProvider = canMoveFiles,
            canDelete = canDeleteFiles,
            canCopyDirectoriesWithinProvider = true,
            canMoveDirectoriesWithinProvider = true,
        ),
    )
}

private class MemorySkippedWriteSession(private val existing: EntryRef) : AtomicWriteSession {
    override val temporaryRef = existing
    override val plannedFinalName = existing.opaqueId.substringAfterLast('/')
    override val replacementBackupName: String? = null
    override val skippedExistingRef = existing
    override val output = object : OutputStream() { override fun write(value: Int) = Unit }
    override suspend fun verify(expected: WriteVerification) = true
    override suspend fun commit() = existing
    override suspend fun abort() = Unit
}
