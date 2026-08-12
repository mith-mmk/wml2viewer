package io.github.mith_mmk.wml2viewer.data.config

import io.github.mith_mmk.wml2viewer.data.config.proto.SourceProfileV1
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

/** Persists only source descriptors; SMB secret material stays in KeystoreCredentialStore. */
interface SavedSourceProfileStore {
    suspend fun current(): List<SourceProfileV1>
    suspend fun upsert(profile: SourceProfileV1)
    suspend fun remove(sourceId: String): Boolean
}

class MobileSourceProfileStore(
    private val repository: MobileConfigRepository,
) : SavedSourceProfileStore {
    val profiles: Flow<List<SourceProfileV1>> = repository.config.map { it.sourcesList }

    override suspend fun current(): List<SourceProfileV1> = repository.current().sourcesList

    override suspend fun upsert(profile: SourceProfileV1) {
        require(profile.sourceId.isNotBlank()) { "sourceId must not be blank" }
        require(profile.sourceCase != SourceProfileV1.SourceCase.SOURCE_NOT_SET) {
            "source provider descriptor is required"
        }
        repository.update { current ->
            val retained = current.sourcesList.filterNot { it.sourceId == profile.sourceId }
            current.toBuilder()
                .clearSources()
                .addAllSources(retained + profile)
                .build()
        }
    }

    override suspend fun remove(sourceId: String): Boolean {
        var removed = false
        repository.update { current ->
            val retained = current.sourcesList.filterNot {
                (it.sourceId == sourceId).also { matches -> removed = removed || matches }
            }
            current.toBuilder().clearSources().addAllSources(retained).build()
        }
        return removed
    }
}
