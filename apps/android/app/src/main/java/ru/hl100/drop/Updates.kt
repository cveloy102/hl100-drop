package ru.hl100.drop

import android.content.Context
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest

data class ReleaseInfo(val version:String,val notes:String,val url:String,val digest:String,val checksums:String?)
object Updates {
    private const val repo="cveloy102/hl100-drop"
    private fun connection(url:String):HttpURLConnection=(URL(url).openConnection() as HttpURLConnection).apply { connectTimeout=10000;readTimeout=30000;setRequestProperty("User-Agent","HL100Drop/${BuildConfig.VERSION_NAME}") }
    private fun text(url:String):String { val c=connection(url);try {return c.inputStream.bufferedReader().use { it.readText() }}finally{c.disconnect()} }
    fun check():ReleaseInfo? {
        val c=connection("https://api.github.com/repos/$repo/releases/latest")
        val json=try{if(c.responseCode==404)return null;JSONObject(c.inputStream.bufferedReader().use{it.readText()})}finally{c.disconnect()}
        val version=json.getString("tag_name").removePrefix("v")
        if(!newer(version,BuildConfig.VERSION_NAME))return null
        val assets=json.getJSONArray("assets");var apk:JSONObject?=null;var sums:String?=null
        for(i in 0 until assets.length()){val a=assets.getJSONObject(i);when(a.getString("name")){"HL100Drop-$version.apk"->apk=a;"checksums.txt"->sums=a.getString("browser_download_url")}}
        val a=apk?:return null
        return ReleaseInfo(version,json.optString("body"),a.getString("browser_download_url"),a.optString("digest",""),sums)
    }
    private fun newer(a:String,b:String):Boolean { val aa=a.split('.').map { it.toIntOrNull()?:return false };val bb=b.split('.').map { it.toIntOrNull()?:return false };for(i in 0 until maxOf(aa.size,bb.size)){val diff=aa.getOrElse(i){0}-bb.getOrElse(i){0};if(diff!=0)return diff>0};return false }
    fun download(c:Context,r:ReleaseInfo,progress:(Int)->Unit):File {
        validate(r.url);val name="HL100Drop-${r.version}.apk";var digest=r.digest.removePrefix("sha256:")
        if(digest.length!=64){val url=r.checksums?:throw IllegalStateException("В релизе нет контрольной суммы");validate(url);digest=text(url).lineSequence().map { it.trim().split(Regex("\\s+")) }.firstOrNull{it.size==2&&it[1].removePrefix("*")==name}?.first()?:throw IllegalStateException("Нет контрольной суммы APK")}
        val folder=File(c.cacheDir,"updates").apply{mkdirs()};val file=File(folder,name);val partial=File(folder,"$name.part");val conn=connection(r.url)
        try{val hash=MessageDigest.getInstance("SHA-256");val total=conn.contentLengthLong;var done=0L;conn.inputStream.use { input->partial.outputStream().use { output->val buffer=ByteArray(1024*1024);while(true){val n=input.read(buffer);if(n<0)break;output.write(buffer,0,n);hash.update(buffer,0,n);done+=n;progress(if(total>0)(done*100/total).toInt() else 0)} } };val actual=hash.digest().joinToString(""){"%02x".format(it)};check(actual.equals(digest,true)){"Контрольная сумма обновления не совпала"};if(file.exists())file.delete();check(partial.renameTo(file)){"Не удалось сохранить обновление"};return file}finally{partial.delete();conn.disconnect()}
    }
    private fun validate(url:String){val u=URL(url);require(u.protocol=="https"&&u.host=="github.com"&&u.path.startsWith("/$repo/releases/download/")){"Некорректный адрес обновления"}}
}
