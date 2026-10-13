package ru.hl100.drop

import android.content.Context
import android.net.Uri
import androidx.documentfile.provider.DocumentFile
import org.json.JSONObject
import java.io.IOException

class NativeBridge(context: Context, private val tree: Uri) {
    private val context = context.applicationContext
    companion object { init { System.loadLibrary("hl100_core") } }
    external fun command(json: String): String
    private val roots = mutableMapOf<String, Pair<DocumentFile,String>>()
    private val partials = mutableMapOf<String, Pair<DocumentFile,String>>()
    fun call(value: JSONObject): JSONObject = synchronized(this) { JSONObject(command(value.toString())) }

    fun openSource(uri: String): Int = context.contentResolver.openFileDescriptor(Uri.parse(uri), "r")?.detachFd() ?: throw IOException("Исходный файл недоступен")
    @Synchronized
    fun openReceive(transfer: String, path: String, directory: Boolean): Int {
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
        if(directory) { if(parent.findFile(name)==null && parent.createDirectory(name)==null) throw IOException("Не удалось создать папку");return -1 }
        val temp=parent.createFile("application/octet-stream","$name.$transfer.part") ?: throw IOException("Не удалось создать файл")
        partials["$transfer:$path"]=temp to name
        return context.contentResolver.openFileDescriptor(temp.uri,"w")?.detachFd() ?: throw IOException("Не удалось открыть файл для записи")
    }
    @Synchronized
    fun finishReceive(transfer: String,path: String) {
        val key="$transfer:$path";val (file,name)=partials[key] ?: throw IOException("Не найден полученный файл")
        if(!file.renameTo(name)) throw IOException("Не удалось завершить сохранение файла")
        partials.remove(key)
    }
    @Synchronized
    fun abortReceive(transfer: String) {
        val prefix="$transfer:"
        partials.keys.filter { it.startsWith(prefix) }.forEach { partials.remove(it)?.first?.delete() }
        roots.keys.filter { it.startsWith(prefix) }.forEach { roots.remove(it) }
    }
    private fun numbered(name: String,n: Int): String { val i=name.lastIndexOf('.');return if(i>0)name.substring(0,i)+" ($n)"+name.substring(i) else "$name ($n)" }
}
