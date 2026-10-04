package dev.vamsi.planner.data

import android.content.Context
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

/*
 * Google Drive REST calls (blocking; call from Dispatchers.IO) — the same
 * layout as the desktop app's cloud.rs, so both use one file:
 *   My Drive/<folder>/planner-data.json   (folder defaults to "Planner")
 *
 * The token has the `drive.file` scope: Planner only sees files its Google
 * Cloud project created (desktop and Android share the project), and every
 * lookup here is confined to the chosen folder.
 */

const val DRIVE_SCOPE = "https://www.googleapis.com/auth/drive.file"
const val DEFAULT_FOLDER = "Planner"
const val FILE_NAME = "planner-data.json"
private const val API = "https://www.googleapis.com/drive/v3"
private const val UPLOAD_API = "https://www.googleapis.com/upload/drive/v3"
private const val FOLDER_MIME = "application/vnd.google-apps.folder"
private const val META_FIELDS = "id,version,modifiedTime,trashed,parents"

data class DriveMeta(val id: String, val version: String?, val modified: String?, val trashed: Boolean, val parents: List<String>)

class DriveException(message: String, val status: Int = 0) : IOException(message) {
    /** The token was rejected: the user has to sign in again. */
    val isAuth get() = status == 401
}

/** Which folder/file we sync with, plus bookkeeping. Kept in SharedPreferences. */
data class SyncAccount(
    val signedIn: Boolean = false,
    val email: String? = null,
    val folder: String = DEFAULT_FOLDER,
    val fileId: String? = null,
    /** Drive's `version` of the file as of our last upload or download. */
    val version: String? = null,
    /** Local changes not uploaded yet. */
    val dirty: Boolean = false,
    val lastSync: Long? = null,
) {
    fun withFolder(name: String) = copy(folder = name.trim(), fileId = null, version = null)

    fun save(context: Context) {
        context.getSharedPreferences("drive", Context.MODE_PRIVATE).edit()
            .putBoolean("signedIn", signedIn).putString("email", email).putString("folder", folder)
            .putString("fileId", fileId).putString("version", version).putBoolean("dirty", dirty)
            .putLong("lastSync", lastSync ?: 0L)
            .apply()
    }

    companion object {
        fun load(context: Context): SyncAccount {
            val p = context.getSharedPreferences("drive", Context.MODE_PRIVATE)
            return SyncAccount(
                signedIn = p.getBoolean("signedIn", false),
                email = p.getString("email", null),
                folder = p.getString("folder", null) ?: DEFAULT_FOLDER,
                fileId = p.getString("fileId", null),
                version = p.getString("version", null),
                dirty = p.getBoolean("dirty", false),
                lastSync = p.getLong("lastSync", 0L).takeIf { it > 0 },
            )
        }
    }
}

object Drive {
    private val json = Json { ignoreUnknownKeys = true }

    private fun request(token: String, method: String, url: String, body: String? = null, contentType: String = "application/json"): String {
        val conn = URL(url).openConnection() as HttpURLConnection
        try {
            // HttpURLConnection has no PATCH; Google APIs accept the override header.
            conn.requestMethod = if (method == "PATCH") "POST" else method
            if (method == "PATCH") conn.setRequestProperty("X-HTTP-Method-Override", "PATCH")
            conn.connectTimeout = 20_000
            conn.readTimeout = 30_000
            conn.setRequestProperty("Authorization", "Bearer $token")
            if (body != null) {
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "$contentType; charset=utf-8")
                conn.outputStream.use { it.write(body.toByteArray()) }
            }
            val code = conn.responseCode
            val stream = if (code in 200..299) conn.inputStream else conn.errorStream
            val text = stream?.bufferedReader()?.use { it.readText() }.orEmpty()
            if (code !in 200..299) {
                val detail = runCatching {
                    val e = json.parseToJsonElement(text).jsonObject["error"]
                    (e as? JsonObject)?.get("message")?.jsonPrimitive?.content ?: (e as? JsonPrimitive)?.content
                }.getOrNull() ?: text.take(200)
                throw DriveException("Google returned $code: $detail", code)
            }
            return text
        } catch (e: IOException) {
            throw e as? DriveException ?: DriveException("Network error: ${e.message}")
        } finally {
            conn.disconnect()
        }
    }

    private fun enc(s: String) = URLEncoder.encode(s, "UTF-8").replace("+", "%20")

    /** Quote a value as a Drive query string literal. */
    fun quoted(s: String) = "'" + s.replace("\\", "\\\\").replace("'", "\\'") + "'"

    private fun meta(o: JsonObject) = DriveMeta(
        id = o["id"]!!.jsonPrimitive.content,
        version = o["version"]?.jsonPrimitive?.content,
        modified = o["modifiedTime"]?.jsonPrimitive?.content,
        trashed = o["trashed"]?.jsonPrimitive?.content == "true",
        parents = (o["parents"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty(),
    )

    private fun search(token: String, q: String): List<DriveMeta> {
        val url = "$API/files?q=${enc(q)}&spaces=drive&fields=files($META_FIELDS)&orderBy=modifiedTime%20desc&pageSize=10"
        return json.parseToJsonElement(request(token, "GET", url)).jsonObject["files"]!!.jsonArray.map { meta(it.jsonObject) }
    }

    fun userEmail(token: String): String? = runCatching {
        json.parseToJsonElement(request(token, "GET", "$API/about?fields=user(emailAddress)"))
            .jsonObject["user"]!!.jsonObject["emailAddress"]!!.jsonPrimitive.content
    }.getOrNull()

    private fun findFolder(token: String, folder: String): String? =
        search(token, "name = ${quoted(folder)} and mimeType = '$FOLDER_MIME' and 'root' in parents and trashed = false").firstOrNull()?.id

    /** The data file inside the chosen folder, if there is one. */
    fun findFile(token: String, acct: SyncAccount): DriveMeta? {
        val folder = findFolder(token, acct.folder) ?: return null
        acct.fileId?.let { id ->
            runCatching { meta(json.parseToJsonElement(request(token, "GET", "$API/files/$id?fields=$META_FIELDS")).jsonObject) }
                .getOrNull()
                ?.takeIf { !it.trashed && folder in it.parents }
                ?.let { return it }
        }
        return search(token, "name = '$FILE_NAME' and ${quoted(folder)} in parents and trashed = false").firstOrNull()
    }

    fun download(token: String, id: String): String = request(token, "GET", "$API/files/$id?alt=media")

    /** Upload [data], creating the folder and file the first time. */
    fun upload(token: String, acct: SyncAccount, data: String): DriveMeta {
        val id = findFile(token, acct)?.id ?: run {
            val folder = findFolder(token, acct.folder) ?: create(token, buildJsonObject {
                put("name", acct.folder)
                put("mimeType", FOLDER_MIME)
                put("parents", buildJsonArray { add(JsonPrimitive("root")) })
            })
            create(token, buildJsonObject {
                put("name", FILE_NAME)
                put("mimeType", "application/json")
                put("parents", buildJsonArray { add(JsonPrimitive(folder)) })
            })
        }
        val reply = request(token, "PATCH", "$UPLOAD_API/files/$id?uploadType=media&fields=$META_FIELDS", data)
        return meta(json.parseToJsonElement(reply).jsonObject)
    }

    private fun create(token: String, metadata: JsonObject): String =
        json.parseToJsonElement(request(token, "POST", "$API/files?fields=id", metadata.toString())).jsonObject["id"]!!.jsonPrimitive.content
}
