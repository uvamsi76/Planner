package dev.vamsi.planner.ui

import android.app.Activity
import android.app.Application
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.util.Log
import androidx.activity.result.IntentSenderRequest
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import com.google.android.gms.auth.api.identity.AuthorizationRequest
import com.google.android.gms.auth.api.identity.ClearTokenRequest
import com.google.android.gms.auth.api.identity.Identity
import com.google.android.gms.auth.api.identity.RevokeAccessRequest
import com.google.android.gms.common.api.ApiException
import com.google.android.gms.common.api.Scope
import com.google.android.gms.tasks.Task
import dev.vamsi.planner.data.DRIVE_SCOPE
import dev.vamsi.planner.data.Drive
import dev.vamsi.planner.data.DriveException
import dev.vamsi.planner.data.DriveMeta
import dev.vamsi.planner.data.Store
import dev.vamsi.planner.data.SyncAccount
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.io.File
import java.security.MessageDigest
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

sealed interface SyncStatus {
    data object SignedOut : SyncStatus
    data object Syncing : SyncStatus
    data object Synced : SyncStatus
    data class Error(val message: String) : SyncStatus
}

private object NeedsSignIn : Exception("Sign in to Google Drive again")

private suspend fun <T> Task<T>.await(): T = suspendCancellableCoroutine { c ->
    addOnSuccessListener { c.resume(it) }
    addOnFailureListener { c.resumeWithException(it) }
    addOnCanceledListener { c.cancel() }
}

/**
 * Keeps the local file and Google Drive in step — the same rules as the
 * desktop app's sync.rs:
 * - every local save marks the account dirty and uploads ~2 s later;
 * - on start/resume, if Drive's copy changed since we last saw it, download
 *   it — unless we also have unsent changes: then this phone's data wins and
 *   is uploaded over Drive's copy (Drive's old copy is kept as a backup file);
 * - before an upload, if Drive changed underneath us, the same rule applies.
 *
 * Tokens come from Google Play services' Authorization API: the app is
 * identified by its package name + signing certificate (an "Android" OAuth
 * client in the same Google Cloud project as the desktop app). Once granted,
 * `authorize()` returns a fresh token silently.
 */
class DriveSync(
    private val app: Application,
    private val scope: CoroutineScope,
    private val currentStore: () -> Store,
    private val applyStore: (Store) -> Unit,
    private val notify: (String) -> Unit,
) {
    var account by mutableStateOf(SyncAccount.load(app))
        private set
    var status by mutableStateOf<SyncStatus>(if (account.signedIn) SyncStatus.Synced else SyncStatus.SignedOut)
        private set
    var signingIn by mutableStateOf(false)
        private set

    private val mutex = Mutex()
    private var uploadJob: Job? = null
    private val client get() = Identity.getAuthorizationClient(app)
    private val request get() = AuthorizationRequest.builder().setRequestedScopes(listOf(Scope(DRIVE_SCOPE))).build()

    private fun persist(a: SyncAccount) {
        account = a
        a.save(app)
    }

    /** A token without UI; throws [NeedsSignIn] if the user must consent again. */
    private suspend fun token(): String {
        val result = client.authorize(request).await()
        if (result.hasResolution()) throw NeedsSignIn
        return result.accessToken ?: throw DriveException("Google returned no access token")
    }

    // ------------------------------------------------------------ sign-in

    /** Start sign-in; [launch] shows Google's consent screen when needed. */
    fun signIn(folder: String, launch: (IntentSenderRequest) -> Unit) {
        persist(account.withFolder(folder.ifBlank { account.folder }))
        signingIn = true
        try {
            client.authorize(request)
                .addOnSuccessListener { r ->
                    val pending = r.pendingIntent
                    try {
                        if (r.hasResolution() && pending != null) launch(IntentSenderRequest.Builder(pending.intentSender).build())
                        else onAuthorized(r.accessToken)
                    } catch (e: Exception) {
                        fail("launching Google sign-in", e)
                    }
                }
                .addOnFailureListener { e -> fail("authorize", e) }
        } catch (e: Exception) {
            // e.g. Google Play services missing on this device.
            fail("authorize", e)
        }
    }

    private fun fail(step: String, e: Throwable) {
        Log.w(TAG, "Drive sign-in failed at $step", e)
        signingIn = false
        status = SyncStatus.Error(describe(e))
    }

    /** Result of Google's consent screen. */
    fun onConsentResult(resultCode: Int, data: Intent?) {
        // Google reports failures (e.g. an unregistered app) inside the result
        // intent, often with RESULT_CANCELED, so always ask it what happened.
        try {
            if (data == null) {
                signingIn = false
                if (resultCode != Activity.RESULT_OK) status = SyncStatus.Error("Sign-in was cancelled")
                return
            }
            onAuthorized(client.getAuthorizationResultFromIntent(data).accessToken)
        } catch (e: Exception) {
            fail("consent result (resultCode=$resultCode)", e)
        }
    }

    private fun onAuthorized(token: String?) {
        if (token == null) {
            signingIn = false
            status = SyncStatus.Error("Google returned no access token")
            return
        }
        scope.launch {
            val email = withContext(Dispatchers.IO) { Drive.userEmail(token) }
            persist(account.copy(signedIn = true, email = email, version = null, fileId = null))
            signingIn = false
            syncNow()
        }
    }

    fun signOut() {
        val email = account.email
        scope.launch {
            runCatching {
                val token = client.authorize(request).await().accessToken
                if (token != null) client.clearToken(ClearTokenRequest.builder().setToken(token).build()).await()
                if (email != null) {
                    client.revokeAccess(
                        RevokeAccessRequest.builder()
                            .setAccount(android.accounts.Account(email, "com.google"))
                            .setScopes(listOf(Scope(DRIVE_SCOPE)))
                            .build(),
                    ).await()
                }
            }
        }
        persist(SyncAccount(folder = account.folder))
        status = SyncStatus.SignedOut
        notify("Signed out. Your data stays on this phone and in Drive.")
    }

    fun changeFolder(name: String) {
        if (name.isBlank() || name.trim() == account.folder) return
        persist(account.withFolder(name))
        syncNow()
    }

    // ------------------------------------------------------------ syncing

    private sealed interface Outcome {
        data class UpToDate(val meta: DriveMeta) : Outcome
        data class Pushed(val meta: DriveMeta) : Outcome
        data class Pulled(val meta: DriveMeta, val remote: String) : Outcome
        data class Clash(val meta: DriveMeta, val remote: String) : Outcome
    }

    /** Same data, ignoring formatting (desktop and Android format JSON differently). */
    private fun sameData(remote: String, local: Store) = runCatching { Store.fromJson(remote) == local }.getOrDefault(false)

    /** Bring local and Drive in step. */
    fun syncNow() = run { token, acct, store ->
        val data = store.toJson()
        val meta = Drive.findFile(token, acct) ?: return@run Outcome.Pushed(Drive.upload(token, acct, data))
        if (meta.version == acct.version) {
            return@run if (acct.dirty) Outcome.Pushed(Drive.upload(token, acct, data)) else Outcome.UpToDate(meta)
        }
        val remote = Drive.download(token, meta.id)
        val firstTime = acct.version == null
        if ((acct.dirty || (firstTime && store.goals.isNotEmpty())) && !sameData(remote, store)) Outcome.Clash(meta, remote)
        else Outcome.Pulled(meta, remote)
    }

    /** Called after every local save. */
    fun localSaved() {
        if (!account.signedIn) return
        if (!account.dirty) persist(account.copy(dirty = true))
        uploadJob?.cancel()
        uploadJob = scope.launch {
            delay(2_000)
            upload(force = false)
        }
    }

    /** Upload now (e.g. app going to the background) if anything is unsent. */
    fun flush() {
        if (account.signedIn && account.dirty) {
            uploadJob?.cancel()
            upload(force = false)
        }
    }

    private fun upload(force: Boolean) = run { token, acct, store ->
        if (!force && acct.version != null) {
            val meta = Drive.findFile(token, acct)
            if (meta != null && meta.version != acct.version) return@run Outcome.Clash(meta, Drive.download(token, meta.id))
        }
        Outcome.Pushed(Drive.upload(token, acct, store.toJson()))
    }

    private fun run(work: (String, SyncAccount, Store) -> Outcome) {
        if (!account.signedIn) return
        scope.launch {
            mutex.withLock {
                status = SyncStatus.Syncing
                val acct = account
                val store = currentStore()
                try {
                    val token = token()
                    when (val o = withContext(Dispatchers.IO) { work(token, acct, store) }) {
                        is Outcome.UpToDate -> markSynced(o.meta)
                        is Outcome.Pushed -> markSynced(o.meta)
                        is Outcome.Pulled -> {
                            applyRemote(o.remote)
                            markSynced(o.meta)
                        }
                        is Outcome.Clash -> keepLocal(o.meta, o.remote)
                    }
                } catch (e: Exception) {
                    Log.w(TAG, "Drive sync failed", e)
                    if (e === NeedsSignIn || (e is DriveException && e.isAuth)) {
                        persist(SyncAccount(folder = acct.folder, dirty = acct.dirty))
                        status = SyncStatus.SignedOut
                        notify("Google Drive sign-in expired. Sign in again to keep syncing.")
                    } else {
                        status = SyncStatus.Error(describe(e))
                    }
                }
            }
        }
    }

    private fun markSynced(meta: DriveMeta) {
        persist(account.copy(fileId = meta.id, version = meta.version, dirty = false, lastSync = System.currentTimeMillis()))
        status = SyncStatus.Synced
    }

    /** Replace local data with Drive's copy, keeping a backup of the local file. */
    private fun applyRemote(remote: String) {
        val store = try {
            Store.fromJson(remote)
        } catch (e: Exception) {
            status = SyncStatus.Error("Drive file isn't Planner data: ${e.message}")
            return
        }
        File(app.filesDir, "data.before-drive.json").writeText(currentStore().toJson())
        applyStore(store)
    }

    /**
     * Both sides changed: this phone's data wins and is uploaded over Drive's
     * copy (no prompt). Drive's previous copy is kept as a local backup file.
     */
    private fun keepLocal(meta: DriveMeta, remote: String) {
        runCatching { File(app.filesDir, "data.drive-backup.json").writeText(remote) }
            .onFailure { Log.w(TAG, "Couldn't back up Drive's copy", it) }
        persist(account.copy(version = meta.version))
        upload(force = true)
    }

    private fun describe(e: Throwable): String = when (e) {
        is ApiException -> when (e.statusCode) {
            // DEVELOPER_ERROR: this package + signing key isn't registered as an Android OAuth client.
            10 -> "Google doesn't recognise this app yet. In Google Cloud Console → Clients, create an " +
                "Android client with:\nPackage name: ${app.packageName}\nSHA-1: ${signingSha1() ?: "unknown"}\n" +
                "(same project as the desktop client, with the Google Drive API enabled)."
            7 -> "No internet connection."
            16, 12501 -> "Sign-in was cancelled."
            1, 2, 3, 9, 17, 18 -> "Google Play services is missing or out of date on this device (code ${e.statusCode})."
            else -> "Google sign-in failed (code ${e.statusCode}): ${e.message ?: ""}".trimEnd(' ', ':')
        }
        else -> e.message ?: e.toString()
    }

    /** SHA-1 of the certificate this installed app is signed with, as Google Cloud wants it. */
    fun signingSha1(): String? = runCatching {
        val pm = app.packageManager
        val cert = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            pm.getPackageInfo(app.packageName, PackageManager.GET_SIGNING_CERTIFICATES).signingInfo?.apkContentsSigners?.firstOrNull()
        } else {
            @Suppress("DEPRECATION")
            pm.getPackageInfo(app.packageName, PackageManager.GET_SIGNATURES).signatures?.firstOrNull()
        } ?: return null
        MessageDigest.getInstance("SHA-1").digest(cert.toByteArray()).joinToString(":") { "%02X".format(it) }
    }.getOrNull()

    private companion object {
        const val TAG = "PlannerDrive"
    }
}
