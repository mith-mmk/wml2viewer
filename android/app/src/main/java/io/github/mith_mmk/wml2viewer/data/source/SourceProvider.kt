package io.github.mith_mmk.wml2viewer.data.source

interface SourceProvider : AutoCloseable {
    val providerId: String
    val root: EntryRef
    val capabilities: SourceCapabilities

    suspend fun list(parent: EntryRef): List<SourceEntry>
    suspend fun stat(entry: EntryRef): SourceEntry
    suspend fun openRead(entry: EntryRef): SourceRead
    suspend fun create(parent: EntryRef, request: CreateRequest): AtomicWriteSession

    suspend fun createDirectory(
        parent: EntryRef,
        name: String,
        collisionPolicy: CollisionPolicy = CollisionPolicy.FAIL,
    ): EntryRef

    suspend fun copy(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef

    suspend fun move(
        source: EntryRef,
        destinationParent: EntryRef,
        destinationName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef

    suspend fun rename(
        entry: EntryRef,
        newName: String,
        collisionPolicy: CollisionPolicy,
    ): EntryRef

    suspend fun trashOrDelete(entry: EntryRef, allowPermanentDelete: Boolean): DeleteDisposition

    suspend fun thumbnail(entry: EntryRef, maxWidth: Int, maxHeight: Int): SourceThumbnail?

    override fun close() = Unit
}

class SourceProviderRegistry(providers: Iterable<SourceProvider> = emptyList()) : AutoCloseable {
    private val providers = LinkedHashMap<String, SourceProvider>()

    init {
        providers.forEach(::register)
    }

    @Synchronized
    fun register(provider: SourceProvider) {
        require(provider.providerId !in providers) { "Provider already registered: ${provider.providerId}" }
        providers[provider.providerId] = provider
    }

    @Synchronized
    fun unregister(providerId: String): SourceProvider? = providers.remove(providerId)

    @Synchronized
    fun require(ref: EntryRef): SourceProvider = providers[ref.providerId]
        ?: throw SourceException(SourceErrorCode.INVALID_REFERENCE, "Source provider is unavailable")

    @Synchronized
    override fun close() {
        providers.values.forEach(SourceProvider::close)
        providers.clear()
    }
}
