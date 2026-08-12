package io.github.mith_mmk.wml2viewer.data.source

import com.google.common.truth.Truth.assertThat
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertThrows
import org.junit.Test
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.OutputStream
import java.security.MessageDigest

class CrossProviderDirectoryTransferTest {
    @Test
    fun recoveryChecksOnlyThePersistedPlannedName() = runTest {
        val destination = TreeTestProvider("destination")
        val bytes = byteArrayOf(1, 2, 3)
        destination.putFile("book.cbz", bytes)
        destination.putFile("book (2).cbz", bytes)
        val expected = ByteArrayInputStream(bytes).sha256AndSize()
        val engine = engine(destination)

        assertThat(engine.findVerifiedDestination(destination.root, "book (3).cbz", expected)).isNull()
        assertThat(engine.findVerifiedDestination(destination.root, "book (2).cbz", expected))
            .isEqualTo(destination.ref("book (2).cbz"))
    }

    @Test
    fun recoveryCanDistinguishLiveStagingFromPublishedEntry() = runTest {
        val destination = TreeTestProvider("destination")
        val staging = destination.putFile(".wml2viewer.part", byteArrayOf(1))
        val engine = engine(destination)

        assertThat(engine.entryExists(staging)).isTrue()
        destination.trashOrDelete(staging, allowPermanentDelete = true)
        assertThat(engine.entryExists(staging)).isFalse()
    }

    @Test
    fun recoveryRecognizesProviderThatKeepsOpaqueIdAfterPublishRename() = runTest {
        val destination = TreeTestProvider("destination")
        val bytes = byteArrayOf(4, 5, 6)
        val staging = destination.putFile(".wml2viewer.part", bytes)
        val expected = ByteArrayInputStream(bytes).sha256AndSize()

        val published = destination.rename(staging, "page.bin", CollisionPolicy.FAIL)
        val engine = engine(destination)

        assertThat(published).isEqualTo(staging)
        assertThat(engine.statOrNull(staging)?.name).isEqualTo("page.bin")
        assertThat(engine.verifyEntry(staging, expected)).isTrue()
        assertThat(destination.bytes("page.bin")).isEqualTo(bytes)
    }

    @Test
    fun recoveryRestoresTheJournaledReplacementBackupBeforeRetry() = runTest {
        val destination = TreeTestProvider("destination")
        val original = destination.putFile("page.bin", byteArrayOf(9, 8, 7))
        val backup = destination.rename(original, ".wml2viewer-backup-operation", CollisionPolicy.FAIL)
        val engine = engine(destination)

        val restored = engine.restoreReplacementBackup(backup, "page.bin")

        assertThat(destination.nameOf(restored)).isEqualTo("page.bin")
        assertThat(destination.bytes("page.bin")).isEqualTo(byteArrayOf(9, 8, 7))
        assertThat(destination.exists(".wml2viewer-backup-operation")).isFalse()
    }

    @Test
    fun integrityRecoverySurfacesBackupWithoutMutatingUnverifiedVisibleFile() = runTest {
        val destination = TreeTestProvider("destination")
        val originalBytes = byteArrayOf(9, 8, 7)
        val corruptBytes = byteArrayOf(1, 1, 1)
        val original = destination.putFile("page.bin", originalBytes)
        val backup = destination.rename(original, ".wml2viewer-backup-operation", CollisionPolicy.FAIL)
        destination.putFile("page.bin", corruptBytes)
        val engine = engine(destination)

        val recovered = engine.surfaceReplacementBackup(backup, "page.bin", "job-123456789")

        assertThat(destination.nameOf(recovered)).isEqualTo("page (recovered-job12345).bin")
        assertThat(destination.bytes("page.bin")).isEqualTo(corruptBytes)
        assertThat(destination.bytes("page (recovered-job12345).bin")).isEqualTo(originalBytes)
        assertThat(destination.exists(".wml2viewer-backup-operation")).isFalse()
    }

    @Test
    fun copyBuildsVerifiedTreeAndKeepsSource() = runTest {
        val source = sourceTree()
        val destination = TreeTestProvider("destination")
        val engine = engine(source, destination)

        val result = engine.transfer(
            source.ref("series"),
            destination.root,
            "copied-series",
            CollisionPolicy.FAIL,
            TransferOperation.COPY,
        )

        assertThat(destination.bytes("copied-series/a.bin")).isEqualTo(byteArrayOf(1, 2, 3))
        assertThat(destination.bytes("copied-series/nested/b.bin")).isEqualTo(byteArrayOf(4, 5))
        assertThat(source.exists("series/a.bin")).isTrue()
        assertThat(result.byteCount).isEqualTo(5)
        assertThat(result.sha256).hasLength(64)
        assertThat(result.sourceDeleted).isFalse()
        assertThat(destination.readPaths.count { it.endsWith("a.bin") }).isAtLeast(2)
        assertThat(destination.readPaths.count { it.endsWith("b.bin") }).isAtLeast(2)
        assertThat(
            engine.findVerifiedDestination(
                destination.root,
                "copied-series",
                WriteVerification(result.byteCount, result.sha256),
            ),
        ).isEqualTo(result.destination)
    }

    @Test
    fun sameProviderDirectoryMoveUsesTheJournaledStagingPath() = runTest {
        val provider = sourceTree()

        val result = engine(provider).transfer(
            provider.ref("series"),
            provider.root,
            "renamed-series",
            CollisionPolicy.FAIL,
            TransferOperation.MOVE,
        )

        assertThat(result.sourceDeleted).isTrue()
        assertThat(provider.exists("series")).isFalse()
        assertThat(provider.bytes("renamed-series/a.bin")).isEqualTo(byteArrayOf(1, 2, 3))
        assertThat(provider.bytes("renamed-series/nested/b.bin")).isEqualTo(byteArrayOf(4, 5))
    }

    @Test
    fun directoryReplaceJournalsAndCleansItsExactBackup() = runTest {
        val source = sourceTree()
        val destination = TreeTestProvider("destination").apply {
            mkdir("series")
            putFile("series/old.bin", byteArrayOf(9))
        }
        val journaledNames = arrayOfNulls<String>(2)

        engine(source, destination).transfer(
            source.ref("series"),
            destination.root,
            "series",
            CollisionPolicy.REPLACE,
            TransferOperation.COPY,
            journal = object : TransferJournal {
                override suspend fun copying(
                    temporary: EntryRef,
                    plannedFinalName: String,
                    replacementBackupName: String?,
                ) {
                    journaledNames[0] = plannedFinalName
                    journaledNames[1] = replacementBackupName
                }
                override suspend fun verifying(verification: WriteVerification) = Unit
                override suspend fun committed(destination: EntryRef) = Unit
                override suspend fun deletingSource() = Unit
            },
        )

        assertThat(journaledNames[0]).isEqualTo("series")
        assertThat(journaledNames[1]).startsWith(".wml2viewer-backup-")
        assertThat(destination.bytes("series/a.bin")).isEqualTo(byteArrayOf(1, 2, 3))
        assertThat(destination.childNames(destination.root).none { it.startsWith(".wml2viewer-backup-") }).isTrue()
    }

    @Test
    fun moveDeletesSourceOnlyAfterDestinationCommitAndFullVerification() = runTest {
        val events = mutableListOf<String>()
        val source = sourceTree().apply {
            beforeDelete = {
                assertThat(events).contains("commit")
                events += "delete"
            }
        }
        val destination = TreeTestProvider("destination").apply {
            afterRead = { events += "verify:$it" }
            afterMove = { events += "commit" }
        }

        val result = engine(source, destination).transfer(
            source.ref("series"),
            destination.root,
            "moved-series",
            CollisionPolicy.FAIL,
            TransferOperation.MOVE,
        )

        assertThat(result.sourceDeleted).isTrue()
        assertThat(source.exists("series")).isFalse()
        assertThat(destination.exists("moved-series/nested/b.bin")).isTrue()
        val commitIndex = events.indexOf("commit")
        assertThat(commitIndex).isGreaterThan(0)
        assertThat(events.take(commitIndex).count { it.startsWith("verify:") }).isAtLeast(4)
        assertThat(events.drop(commitIndex + 1).any { it == "delete" }).isTrue()
    }

    @Test
    fun destinationVerificationFailureRollsBackEveryCreatedEntryAndKeepsSource() {
        val source = sourceTree()
        val destination = TreeTestProvider("destination").apply { failReadName = "b.bin" }

        val error = assertThrows(SourceException::class.java) {
            runTest {
                engine(source, destination).transfer(
                    source.ref("series"),
                    destination.root,
                    "copy",
                    CollisionPolicy.FAIL,
                    TransferOperation.MOVE,
                )
            }
        }

        assertThat(error.code).isEqualTo(SourceErrorCode.IO)
        assertThat(source.exists("series/a.bin")).isTrue()
        assertThat(source.exists("series/nested/b.bin")).isTrue()
        assertThat(destination.childNames(destination.root)).isEmpty()
    }

    @Test
    fun cancellationRollsBackStagingTreeAndKeepsSource() {
        val source = sourceTree()
        val destination = TreeTestProvider("destination")
        var cancel = false

        val error = assertThrows(SourceException::class.java) {
            runTest {
                engine(source, destination).transfer(
                    source.ref("series"),
                    destination.root,
                    "copy",
                    CollisionPolicy.FAIL,
                    TransferOperation.COPY,
                    isCancelled = { cancel },
                    onProgress = { progress -> cancel = progress.bytesCopied >= 4 },
                )
            }
        }

        assertThat(error.code).isEqualTo(SourceErrorCode.CANCELLED)
        assertThat(source.exists("series")).isTrue()
        assertThat(destination.childNames(destination.root)).isEmpty()
    }

    @Test
    fun movePreflightRejectsUndeletableDescendantBeforeCreatingDestination() {
        val source = sourceTree().apply { undeletableNames += "b.bin" }
        val destination = TreeTestProvider("destination")

        val error = assertThrows(SourceException::class.java) {
            runTest {
                engine(source, destination).transfer(
                    source.ref("series"),
                    destination.root,
                    "copy",
                    CollisionPolicy.FAIL,
                    TransferOperation.MOVE,
                )
            }
        }

        assertThat(error.code).isEqualTo(SourceErrorCode.UNSUPPORTED)
        assertThat(source.exists("series/nested/b.bin")).isTrue()
        assertThat(destination.childNames(destination.root)).isEmpty()
    }

    @Test
    fun collisionPoliciesAreAppliedAtAtomicFinalizeBoundary() = runTest {
        val source = sourceTree()
        val destination = TreeTestProvider("destination").apply {
            mkdir("series")
            putFile("series/old.bin", byteArrayOf(9))
        }
        val engine = engine(source, destination)

        val skipped = engine.transfer(
            source.ref("series"), destination.root, "series", CollisionPolicy.SKIP, TransferOperation.MOVE,
        )
        assertThat(skipped.skipped).isTrue()
        assertThat(source.exists("series")).isTrue()
        assertThat(destination.bytes("series/old.bin")).isEqualTo(byteArrayOf(9))

        val kept = engine.transfer(
            source.ref("series"), destination.root, "series", CollisionPolicy.KEEP_BOTH, TransferOperation.COPY,
        )
        assertThat(destination.nameOf(kept.destination)).isEqualTo("series (2)")
        assertThat(destination.bytes("series/old.bin")).isEqualTo(byteArrayOf(9))

        destination.failFinalize = true
        val replaceError = runCatching {
            engine.transfer(
                source.ref("series"), destination.root, "series", CollisionPolicy.REPLACE, TransferOperation.MOVE,
            )
        }.exceptionOrNull()
        assertThat(replaceError).isInstanceOf(SourceException::class.java)
        assertThat(source.exists("series/nested/b.bin")).isTrue()
        assertThat(destination.bytes("series/old.bin")).isEqualTo(byteArrayOf(9))
        assertThat(destination.childNames(destination.root)).containsExactly("series", "series (2)")
    }

    private fun sourceTree() = TreeTestProvider("source").apply {
        mkdir("series")
        putFile("series/a.bin", byteArrayOf(1, 2, 3))
        mkdir("series/nested")
        putFile("series/nested/b.bin", byteArrayOf(4, 5))
    }

    private fun engine(vararg providers: SourceProvider) = SourceTransferEngine(
        SourceProviderRegistry(providers.asList()),
        bufferSize = 4 * 1024,
    )
}

internal class TreeTestProvider(override val providerId: String) : SourceProvider {
    private data class Node(
        val id: String,
        var parentId: String?,
        var name: String,
        val kind: EntryKind,
        var bytes: ByteArray? = null,
    )

    private val nodes = linkedMapOf("root" to Node("root", null, "root", EntryKind.DIRECTORY))
    private var nextId = 1
    override val root = EntryRef(providerId, "root")
    override val capabilities = SourceCapabilities(
        canList = true,
        canRead = true,
        canCreate = true,
        canCopyWithinProvider = true,
        canMoveWithinProvider = true,
        canRename = true,
        canDelete = true,
        hasAtomicFinalize = true,
        canCopyDirectoriesWithinProvider = true,
        canMoveDirectoriesWithinProvider = true,
        canTransferDirectoriesAcrossProviders = true,
    )
    val readPaths = mutableListOf<String>()
    var failReadName: String? = null
    var failFinalize = false
    var afterRead: ((String) -> Unit)? = null
    var afterMove: (() -> Unit)? = null
    var beforeDelete: ((EntryRef) -> Unit)? = null
    val undeletableNames = mutableSetOf<String>()

    fun mkdir(path: String): EntryRef {
        var parent = root
        path.split('/').filter(String::isNotEmpty).forEach { name ->
            parent = child(parent, name)?.let(::ref) ?: createDirectoryNow(parent, name)
        }
        return parent
    }

    fun putFile(path: String, bytes: ByteArray): EntryRef {
        val parentPath = path.substringBeforeLast('/', "")
        val parent = if (parentPath.isEmpty()) root else mkdir(parentPath)
        val name = path.substringAfterLast('/')
        child(parent, name)?.let(::deleteRecursively)
        val node = Node(newId(), parent.opaqueId, name, EntryKind.FILE, bytes.copyOf())
        nodes[node.id] = node
        return ref(node)
    }

    fun ref(path: String): EntryRef = ref(requireNode(path))
    fun exists(path: String): Boolean = findNode(path) != null
    fun bytes(path: String): ByteArray = requireNotNull(requireNode(path).bytes).copyOf()
    fun nameOf(ref: EntryRef): String = node(ref).name
    fun childNames(parent: EntryRef): List<String> = children(parent).map { it.name }.sorted()

    override suspend fun list(parent: EntryRef): List<SourceEntry> = children(parent)
        .sortedWith(compareBy<Node>({ it.name }, { it.id }))
        .map(::entry)

    override suspend fun stat(entry: EntryRef): SourceEntry = entry(node(entry))

    override suspend fun openRead(entry: EntryRef): SourceRead {
        val node = node(entry)
        if (node.kind != EntryKind.FILE) throw SourceException(SourceErrorCode.UNSUPPORTED, "not a file")
        val path = path(node)
        readPaths += path
        afterRead?.invoke(path)
        if (node.name == failReadName) throw SourceException(SourceErrorCode.IO, "injected read failure")
        val bytes = requireNotNull(node.bytes)
        return SourceRead(ByteArrayInputStream(bytes), bytes.size.toLong(), "application/octet-stream")
    }

    override suspend fun create(parent: EntryRef, request: CreateRequest): AtomicWriteSession {
        require(request.kind == EntryKind.FILE)
        val existing = children(parent).associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(request.name, request.collisionPolicy) { it in existing }) {
            CollisionResolution.Skip -> TreeSkippedSession(ref(existing.getValue(request.name)), request.name)
            is CollisionResolution.Use -> TreeWriteSession(
                parent,
                resolution.name,
                if (resolution.replaceExisting) existing[request.name] else null,
            )
        }
    }

    override suspend fun createDirectory(
        parent: EntryRef,
        name: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef {
        val existing = children(parent).associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(name, collisionPolicy) { it in existing }) {
            CollisionResolution.Skip -> ref(existing.getValue(name))
            is CollisionResolution.Use -> {
                if (resolution.replaceExisting) existing[name]?.let(::deleteRecursively)
                createDirectoryNow(parent, resolution.name)
            }
        }
    }

    override suspend fun copy(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef = throw SourceException(SourceErrorCode.UNSUPPORTED, "not needed")

    override suspend fun move(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef {
        if (failFinalize) throw SourceException(SourceErrorCode.IO, "injected finalize failure")
        val sourceNode = node(source)
        val existing = children(destinationParent).associateBy { it.name }
        return when (val resolution = CollisionResolver.resolve(destinationName, collisionPolicy) { it in existing }) {
            CollisionResolution.Skip -> ref(existing.getValue(destinationName))
            is CollisionResolution.Use -> {
                if (resolution.replaceExisting) existing[destinationName]?.let(::deleteRecursively)
                sourceNode.parentId = destinationParent.opaqueId
                sourceNode.name = resolution.name
                afterMove?.invoke()
                ref(sourceNode)
            }
        }
    }

    override suspend fun rename(entry: EntryRef, newName: String, collisionPolicy: CollisionPolicy): EntryRef {
        val node = node(entry)
        return move(entry, EntryRef(providerId, requireNotNull(node.parentId)), newName, collisionPolicy)
    }

    override suspend fun trashOrDelete(entry: EntryRef, allowPermanentDelete: Boolean): DeleteDisposition {
        val node = node(entry)
        if (node.kind == EntryKind.DIRECTORY && children(entry).isNotEmpty()) {
            throw SourceException(SourceErrorCode.IO, "directory is not empty")
        }
        beforeDelete?.invoke(entry)
        nodes.remove(node.id)
        return DeleteDisposition.PERMANENTLY_DELETED
    }

    override suspend fun thumbnail(entry: EntryRef, maxWidth: Int, maxHeight: Int): SourceThumbnail? = null

    private fun createDirectoryNow(parent: EntryRef, name: String): EntryRef {
        val node = Node(newId(), parent.opaqueId, name, EntryKind.DIRECTORY)
        nodes[node.id] = node
        return ref(node)
    }

    private fun children(parent: EntryRef): List<Node> = nodes.values.filter { it.parentId == parent.opaqueId }
    private fun child(parent: EntryRef, name: String): Node? = children(parent).firstOrNull { it.name == name }
    private fun ref(node: Node) = EntryRef(providerId, node.id)
    private fun node(ref: EntryRef): Node {
        if (ref.providerId != providerId) throw SourceException(SourceErrorCode.INVALID_REFERENCE, "wrong provider")
        return nodes[ref.opaqueId] ?: throw SourceException(SourceErrorCode.NOT_FOUND, "missing")
    }

    private fun findNode(path: String): Node? {
        var current = nodes.getValue("root")
        for (part in path.split('/').filter(String::isNotEmpty)) {
            current = child(ref(current), part) ?: return null
        }
        return current
    }

    private fun requireNode(path: String): Node = findNode(path) ?: error("Missing test node: $path")

    private fun path(node: Node): String {
        if (node.id == "root") return ""
        val parts = ArrayDeque<String>()
        var current = node
        while (current.id != "root") {
            parts.addFirst(current.name)
            current = nodes.getValue(requireNotNull(current.parentId))
        }
        return parts.joinToString("/")
    }

    private fun entry(node: Node) = SourceEntry(
        ref = ref(node),
        parent = node.parentId?.let { EntryRef(providerId, it) },
        name = node.name,
        kind = node.kind,
        mimeType = if (node.kind == EntryKind.FILE) "application/octet-stream" else null,
        size = node.bytes?.size?.toLong(),
        modifiedAtEpochMillis = 1,
        effectiveCapabilities = SourceCapabilities(
            canList = node.kind == EntryKind.DIRECTORY,
            canRead = node.kind == EntryKind.FILE,
            canCreate = node.kind == EntryKind.DIRECTORY,
            canCopyWithinProvider = true,
            canMoveWithinProvider = node.id != "root",
            canRename = node.id != "root",
            canDelete = node.id != "root" && node.name !in undeletableNames,
            hasAtomicFinalize = true,
            canCopyDirectoriesWithinProvider = node.kind == EntryKind.DIRECTORY,
            canMoveDirectoriesWithinProvider = node.kind == EntryKind.DIRECTORY && node.id != "root",
            canTransferDirectoriesAcrossProviders = node.kind == EntryKind.DIRECTORY,
        ),
    )

    private fun deleteRecursively(node: Node) {
        children(ref(node)).toList().forEach(::deleteRecursively)
        nodes.remove(node.id)
    }

    private fun newId() = "node-${nextId++}"

    private inner class TreeWriteSession(
        private val parent: EntryRef,
        private val finalName: String,
        private val replaced: Node?,
    ) : AtomicWriteSession {
        private val buffer = ByteArrayOutputStream()
        override val temporaryRef = EntryRef(providerId, "temp-${newId()}")
        override val plannedFinalName = finalName
        override val replacementBackupName: String? = null
        override val output: OutputStream = buffer
        private var verified = false
        private var finished = false

        override suspend fun verify(expected: WriteVerification): Boolean {
            val bytes = buffer.toByteArray()
            val actual = WriteVerification(
                bytes.size.toLong(),
                MessageDigest.getInstance("SHA-256").digest(bytes).toHex(),
            )
            return (actual == expected).also { verified = it }
        }

        override suspend fun commit(): EntryRef {
            check(verified && !finished)
            replaced?.let(::deleteRecursively)
            finished = true
            val node = Node(newId(), parent.opaqueId, finalName, EntryKind.FILE, buffer.toByteArray())
            nodes[node.id] = node
            return ref(node)
        }

        override suspend fun abort() { finished = true }
    }
}

private class TreeSkippedSession(
    private val existing: EntryRef,
    override val plannedFinalName: String,
) : AtomicWriteSession {
    override val temporaryRef = existing
    override val replacementBackupName: String? = null
    override val skippedExistingRef = existing
    override val output = object : OutputStream() { override fun write(value: Int) = Unit }
    override suspend fun verify(expected: WriteVerification) = true
    override suspend fun commit() = existing
    override suspend fun abort() = Unit
}
