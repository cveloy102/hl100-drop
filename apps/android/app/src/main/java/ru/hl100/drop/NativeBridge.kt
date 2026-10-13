package ru.hl100.drop

import android.content.Context
import android.net.Uri
import androidx.documentfile.provider.DocumentFile
import org.json.JSONObject
import org.json.JSONArray
import java.io.IOException

class NativeBridge(context: Context, private val tree: Uri) {
    private val context = context.applicationContext
    companion object { init { System.loadLibrary("hl100_core") } }
    external fun command(json: String): String
    private val roots = mutableMapOf<String, Pair<DocumentFile,String>>()
    private val partials = mutableMapOf<String, Pair<DocumentFile,String>>()
    private val completed = mutableMapOf<String,DocumentFile>()
    private val preferences=context.getSharedPreferences("drop_storage",Context.MODE_PRIVATE)
    init {
        val saved=JSONObject(preferences.getString(tree.toString(),"{}")?:"{}")
        saved.optJSONObject("roots")?.let{values->values.keys().forEach{key->val item=values.getJSONObject(key);DocumentFile.fromTreeUri(context,Uri.parse(item.getString("uri")))?.let{roots[key]=it to item.getString("name")}}}
        saved.optJSONObject("partials")?.let{values->values.keys().forEach{key->val item=values.getJSONObject(key);DocumentFile.fromSingleUri(context,Uri.parse(item.getString("uri")))?.let{partials[key]=it to item.getString("name")}}}
        saved.optJSONObject("completed")?.let{values->values.keys().forEach{key->DocumentFile.fromSingleUri(context,Uri.parse(values.getString(key)))?.let{completed[key]=it}}}
    }
    fun call(value: JSONObject): JSONObject = JSONObject(command(value.toString()))
    fun stateRoot():String = java.io.File(context.filesDir,"transfers/"+tree.toString().hashCode().toUInt().toString(16)).absolutePath
    private fun persist() {
        val rootValues=JSONObject();roots.forEach{(key,pair)->rootValues.put(key,JSONObject().put("uri",pair.first.uri.toString()).put("name",pair.second))}
        val partialValues=JSONObject();partials.forEach{(key,pair)->partialValues.put(key,JSONObject().put("uri",pair.first.uri.toString()).put("name",pair.second))}
        val completeValues=JSONObject();completed.forEach{(key,file)->completeValues.put(key,file.uri.toString())}
        if(!preferences.edit().putString(tree.toString(),JSONObject().put("roots",rootValues).put("partials",partialValues).put("completed",completeValues).toString()).commit())throw IOException("Не удалось сохранить состояние передачи")
    }
    @Synchronized fun resumeStatus(transfer:String,path:String):String {
        val key="$transfer:$path"
        completed[key]?.let{if(!it.exists())throw IOException("Полученный файл удалён");return JSONArray().put(it.length()).put(true).toString()}
        val partial=partials[key]?.first
        return JSONArray().put(if(partial?.exists()==true)partial.length() else 0).put(false).toString()
    }

    fun openSource(uri: String): Int = context.contentResolver.openFileDescriptor(Uri.parse(uri), "r")?.detachFd() ?: throw IOException("Исходный файл недоступен")
    fun openSourceChecked(uri:String,size:Long,modified:Long):Int {
        val doc=DocumentFile.fromSingleUri(context,Uri.parse(uri))?:throw IOException("Исходный файл недоступен")
        if(doc.length()!=size || (modified>0 && doc.lastModified()>0 && doc.lastModified()!=modified))throw IOException("Исходный файл изменился. Отправьте его заново")
        return openSource(uri)
    }
    @Synchronized
    fun openReceive(transfer: String, path: String, directory: Boolean): Int {
        partials["$transfer:$path"]?.first?.takeIf{it.exists()}?.let{return context.contentResolver.openFileDescriptor(it.uri,"rw")?.detachFd()?:throw IOException("Не удалось продолжить запись")}
        val parts=path.split('/')
        val root=DocumentFile.fromTreeUri(context,tree) ?: throw IOException("Папка сохранения недоступна")
        val rootKey="$transfer:${parts.first()}"
        val pair=roots.getOrPut(rootKey) {
            var name=parts.first();var n=1
            while(root.findFile(name)!=null || roots.values.any { it.first.uri==root.uri && it.second==name }) { name=numbered(parts.first(),n++); }
            root to name
        }
        var parent=pair.first
        val names=listOf(pair.second)+parts.drop(1)
        for(name in names.dropLast(1)) parent=parent.findFile(name) ?: parent.createDirectory(name) ?: throw IOException("Не удалось создать папку $name")
        val name=names.last()
        if(directory) { if(parent.findFile(name)==null && parent.createDirectory(name)==null) throw IOException("Не удалось создать папку");persist();return -1 }
        val temp=parent.createFile("application/octet-stream","$name.$transfer.part") ?: throw IOException("Не удалось создать файл")
        partials["$transfer:$path"]=temp to name
        persist()
        return context.contentResolver.openFileDescriptor(temp.uri,"rw")?.detachFd() ?: throw IOException("Не удалось открыть файл для записи")
    }
    @Synchronized
    fun finishReceive(transfer: String,path: String) {
        val key="$transfer:$path";val (file,name)=partials[key] ?: throw IOException("Не найден полученный файл")
        if(!file.renameTo(name)) throw IOException("Не удалось завершить сохранение файла")
        partials.remove(key)
        completed[key]=file;persist()
    }
    @Synchronized
    fun abortReceive(transfer: String) {
        val prefix="$transfer:"
        partials.keys.filter { it.startsWith(prefix) }.forEach { partials.remove(it)?.first?.delete() }
        roots.keys.filter { it.startsWith(prefix) }.forEach { roots.remove(it) }
        completed.keys.filter { it.startsWith(prefix) }.forEach { completed.remove(it) };persist()
    }
    private fun numbered(name: String,n: Int): String { val i=name.lastIndexOf('.');return if(i>0)name.substring(0,i)+" ($n)"+name.substring(i) else "$name ($n)" }
}
