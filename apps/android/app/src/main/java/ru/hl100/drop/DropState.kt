package ru.hl100.drop

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.*
import androidx.documentfile.provider.DocumentFile
import kotlinx.coroutines.*
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.UUID

data class Peer(val id:String,val name:String,val address:String,val port:Int,val platform:String)
data class TransferFile(val path:String,val size:Long,val directory:Boolean)
data class Transfer(val id:String,val name:String,val direction:String,val total:Long,val done:Long=0,val speed:Double=0.0,val status:String="active",val message:String="",val files:List<TransferFile> = emptyList(),val count:Int=0)
object DropState {
    private val scope=CoroutineScope(SupervisorJob()+Dispatchers.Main)
    var bridge:NativeBridge?=null
    var peers by mutableStateOf(listOf<Peer>())
    var transfers by mutableStateOf(listOf<Transfer>())
    var status by mutableStateOf("Поиск устройств…")
    var busy by mutableStateOf(false)
    var name by mutableStateOf("")
    var folder by mutableStateOf("")
    var running by mutableStateOf(false)
    var release by mutableStateOf<ReleaseInfo?>(null)
    var updateProgress by mutableStateOf<String?>(null)
    private var polling:Job?=null
    fun preferences(c:Context)=c.getSharedPreferences("hl100",Context.MODE_PRIVATE)
    suspend fun start(c:Context) {
        val p=preferences(c);name=p.getString("name","")!!;folder=p.getString("folder","")!!
        if(name.isBlank()||folder.isBlank())return
        val id=p.getString("id",null) ?: UUID.randomUUID().toString().also { p.edit().putString("id",it).apply() }
        polling?.cancel()
        try {
            val next=NativeBridge(c.applicationContext,Uri.parse(folder))
            withContext(Dispatchers.IO) { check(next.call(JSONObject().put("op","start").put("config",JSONObject().put("id",id).put("name",name).put("folder",folder)))) }
            bridge=next;running=true;status="Готов к отправке и автоматическому приёму"
            polling=scope.launch { while(isActive) { try { val state=withContext(Dispatchers.IO) { next.call(JSONObject().put("op","poll")) };busy=state.optBoolean("busy");val list=state.getJSONArray("peers");peers=(0 until list.length()).map { val e=list.getJSONObject(it);Peer(e.getString("id"),e.getString("name"),e.getString("address"),e.getInt("port"),e.getString("platform")) }.sortedBy { it.name };val events=state.getJSONArray("events");for(i in 0 until events.length())event(events.getJSONObject(i)) } catch(e:Exception) { status=e.message?:"Ошибка" };delay(200) } }
        }catch(e:Exception){running=false;status="Не удалось запустить: ${e.message}"}
    }
    fun stop(){polling?.cancel();bridge?.call(JSONObject().put("op","stop"));running=false;busy=false;peers=emptyList()}
    private fun event(e:JSONObject) {
        val id=e.optString("transfer")
        when(e.getString("type")) {
            "error" -> status=e.optString("message")
            "transfer" -> {
                val entries=e.optJSONArray("entries")?:JSONArray()
                val files=(0 until entries.length()).map{val entry=entries.getJSONObject(it);TransferFile(entry.getString("path"),entry.getLong("size"),entry.getBoolean("directory"))}
                transfers=(listOf(Transfer(id,e.getString("name"),e.getString("direction"),e.getLong("total"),files=files,count=e.optInt("files",files.size)))+transfers)
                val active=transfers.filter{it.status=="active"};transfers=active+transfers.filter{it.status!="active"}.take((100-active.size).coerceAtLeast(0))
            }
            "progress" -> transfers=transfers.map { if(it.id==id)it.copy(done=e.getLong("done"),speed=e.getDouble("speed")) else it }
            "complete" -> transfers=transfers.map { if(it.id==id)it.copy(status=e.getString("status"),message=e.optString("message")) else it }
        }
    }
    suspend fun send(c:Context,peer:Peer,uris:List<Uri>,tree:Boolean=false) {
        if(updateProgress!=null){status="Дождитесь обновления";return}
        try { status="Подготовка файлов…";withContext(Dispatchers.IO) {
            val items=JSONArray()
            fun add(doc:DocumentFile,path:String) {
                if(!doc.isDirectory && !doc.isFile)throw IOException("Файл недоступен")
                items.put(JSONObject().put("uri",doc.uri.toString()).put("path",path).put("size",if(doc.isDirectory)0 else doc.length()).put("directory",doc.isDirectory))
                if(doc.isDirectory)doc.listFiles().forEach { add(it,"$path/${it.name ?: "file"}") }
            }
            uris.forEach { uri->val doc=if(tree)DocumentFile.fromTreeUri(c,uri) else DocumentFile.fromSingleUri(c,uri);if(doc==null)throw IOException("Файл недоступен");add(doc,doc.name?:"file") }
            check(bridge?.call(JSONObject().put("op","send").put("address",peer.address).put("port",peer.port).put("name",peer.name).put("items",items))?:throw IOException("Приложение не запущено"))
        };busy=true;status="Передача началась" } catch(e:Exception){status=e.message?:"Ошибка отправки"}
    }
    suspend fun cancel(id:String){withContext(Dispatchers.IO){bridge?.call(JSONObject().put("op","cancel").put("transfer",id))}}
    suspend fun checkUpdates(manual:Boolean=false) {
        try{release=withContext(Dispatchers.IO){Updates.check()};if(manual)status=if(release==null)"Установлена актуальная версия" else "Доступна версия ${release!!.version}"}catch(e:Exception){if(manual)status="Не удалось проверить обновления: ${e.message}"}
    }
    private fun check(v:JSONObject){if(v.has("ok")&&!v.getBoolean("ok"))throw IOException(v.optString("error"))}
}
