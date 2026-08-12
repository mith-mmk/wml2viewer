package io.github.mith_mmk.wml2viewer.data.controller

import com.google.common.truth.Truth.assertThat
import java.util.concurrent.Executors
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Test

class ViewerDispatcherTest {
    @Test
    @Suppress("DEPRECATION")
    fun nativeWorkLeavesTheCallingThread() = runBlocking {
        val caller = Thread.currentThread().id
        Executors.newSingleThreadExecutor { work ->
            Thread(work, "wml2viewer-viewer-test")
        }.asCoroutineDispatcher().use { dispatcher ->
            val worker = runOnViewerDispatcher(dispatcher) {
                Thread.currentThread().id to Thread.currentThread().name
            }

            assertThat(worker.first).isNotEqualTo(caller)
            // kotlinx-coroutines debug mode appends " @coroutine#N" to worker names.
            assertThat(worker.second).startsWith("wml2viewer-viewer-test")
        }
    }

    @Test
    fun activeLaneDoesNotWaitForBlockedPrefetchAndCloseReleasesBothSessions() = runBlocking {
        val registry = FakeHandleRegistry()
        Executors.newSingleThreadExecutor { work ->
            Thread(work, "wml2viewer-active-test")
        }.asCoroutineDispatcher().use { activeDispatcher ->
            Executors.newSingleThreadExecutor { work ->
                Thread(work, "wml2viewer-prefetch-test")
            }.asCoroutineDispatcher().use { prefetchDispatcher ->
                val execution = ViewerDecodeExecution(
                    activeDispatcher = activeDispatcher,
                    prefetchDispatcher = prefetchDispatcher,
                    sessionFactory = registry::openSession,
                    cancelSession = FakeSession::cancelCurrent,
                )
                val prefetchStarted = CompletableDeferred<Int>()
                val releasePrefetch = CompletableDeferred<Unit>()
                val blockedPrefetch = async {
                    execution.prefetch { session ->
                        val imageHandle = session.publishImage()
                        prefetchStarted.complete(session.id)
                        try {
                            releasePrefetch.await()
                        } finally {
                            registry.releaseImage(imageHandle)
                        }
                    }
                }

                val prefetchSessionId = withTimeout(1_000L) { prefetchStarted.await() }
                val activeSessionId = withTimeout(1_000L) {
                    execution.active { session -> session.id }
                }

                assertThat(activeSessionId).isNotEqualTo(prefetchSessionId)
                assertThat(releasePrefetch.isCompleted).isFalse()
                assertThat(registry.openSessionCount).isEqualTo(2)
                assertThat(registry.liveImageCount).isEqualTo(1)

                execution.cancelAll()
                assertThat(registry.liveImageCount).isEqualTo(0)
                assertThat(registry.openSessionCount).isEqualTo(1)
                releasePrefetch.complete(Unit)
                blockedPrefetch.await()
                val replacementPrefetchSessionId = execution.prefetch { session -> session.id }
                assertThat(replacementPrefetchSessionId).isNotEqualTo(prefetchSessionId)
                assertThat(registry.openSessionCount).isEqualTo(2)

                execution.close()
                execution.close()
                assertThat(registry.openSessionCount).isEqualTo(0)
                assertThat(registry.liveImageCount).isEqualTo(0)
            }
        }
    }

    private class FakeHandleRegistry {
        private var nextHandle = 1
        private val sessions = LinkedHashSet<Int>()
        private val imageOwners = LinkedHashMap<Int, Int>()

        @get:Synchronized
        val openSessionCount: Int
            get() = sessions.size

        @get:Synchronized
        val liveImageCount: Int
            get() = imageOwners.size

        @Synchronized
        fun openSession(): FakeSession {
            val handle = nextHandle++
            sessions += handle
            return FakeSession(handle, this)
        }

        @Synchronized
        fun publishImage(owner: Int): Int {
            check(owner in sessions)
            val handle = nextHandle++
            imageOwners[handle] = owner
            return handle
        }

        @Synchronized
        fun releaseImage(handle: Int): Boolean = imageOwners.remove(handle) != null

        @Synchronized
        fun cancelSession(handle: Int) {
            imageOwners.entries.removeAll { it.value == handle }
        }

        @Synchronized
        fun closeSession(handle: Int) {
            cancelSession(handle)
            sessions.remove(handle)
        }
    }

    private class FakeSession(
        val id: Int,
        private val registry: FakeHandleRegistry,
    ) : AutoCloseable {
        fun publishImage(): Int = registry.publishImage(id)

        fun cancelCurrent() = registry.cancelSession(id)

        override fun close() = registry.closeSession(id)
    }
}
