package ru.hl100.drop

import android.Manifest
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.*
import java.io.File

private val Ink=Color(0xFF101010)
private val Paper=Color(0xFFE8E5D9)
private val Red=Color(0xFFEF3F3B)
private val Muted=Color(0xFF99998F)
class MainActivity:ComponentActivity() {
    private var selected:Peer?=null
    private var folderPicked:((Uri)->Unit)?=null
    private var pendingApk:File?=null
    private val documents=registerForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris->selected?.let { peer->if(uris.isNotEmpty())lifecycleScope.launch { DropState.send(this@MainActivity,peer,uris) } } }
    private val sendTree=registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri->if(uri!=null)selected?.let { peer->lifecycleScope.launch { DropState.send(this@MainActivity,peer,listOf(uri),true) } } }
    private val saveTree=registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri->if(uri!=null){contentResolver.takePersistableUriPermission(uri,Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION);folderPicked?.invoke(uri)} }
    private val notifyPermission=registerForActivityResult(ActivityResultContracts.RequestPermission()) { }
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        val prefs=DropState.preferences(this)
        DropState.name=prefs.getString("name","")!!;DropState.folder=prefs.getString("folder","")!!
        if(DropState.name.isNotBlank()&&DropState.folder.isNotBlank())enableService()
        lifecycleScope.launch { if(prefs.getBoolean("updates",true))DropState.checkUpdates() }
        setContent {
            MaterialTheme(colorScheme=darkColorScheme(primary=Red,onPrimary=Ink,background=Ink,surface=Ink,onSurface=Paper),typography=Typography(bodyLarge=MaterialTheme.typography.bodyLarge.copy(fontFamily=FontFamily.Monospace),bodyMedium=MaterialTheme.typography.bodyMedium.copy(fontFamily=FontFamily.Monospace),titleLarge=MaterialTheme.typography.titleLarge.copy(fontFamily=FontFamily.Monospace))) {
                Surface(Modifier.fillMaxSize(),color=Ink) {
                    var settings by remember { mutableStateOf(false) }
                    var configured by remember { mutableStateOf(DropState.name.isNotBlank()&&DropState.folder.isNotBlank()) }
                    Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing).verticalScroll(rememberScrollState()).padding(24.dp)) {
                        Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.SpaceBetween) { Text("HL100_ DROP",fontSize=22.sp,fontFamily=FontFamily.Monospace);TextButton(onClick={if(DropState.busy)DropState.status="Дождитесь завершения передач" else settings=true}){Text("НАСТРОЙКИ ↗")} }
                        HorizontalDivider(Modifier.padding(vertical=20.dp),color=Color(0xFF30302D))
                        if(!configured||settings) {
                            SettingsPanel(first=!configured,onSave={name,folder,updates->
                                prefs.edit().putString("name",name).putString("folder",folder).putBoolean("updates",updates).apply();DropState.name=name;DropState.folder=folder;configured=true;settings=false
                                stopService(Intent(this@MainActivity,TransferService::class.java));lifecycleScope.launch {delay(500);enableService()}
                            },onBack={settings=false})
                        } else {
                            DropState.release?.let { release->Column(Modifier.fillMaxWidth().border(1.dp,Red).padding(16.dp)){Text("Доступна версия ${release.version}");Spacer(Modifier.height(8.dp));Action(DropState.updateProgress?:"ОБНОВИТЬСЯ ↗",DropState.updateProgress==null){update()}};Spacer(Modifier.height(20.dp)) }
                            Label("[ 01 / LOCAL CONNECTION ]")
                            Text("Hello, nearby_",fontSize=30.sp,fontFamily=FontFamily.Monospace,modifier=Modifier.padding(vertical=16.dp))
                            Text(DropState.name,color=Paper);Text(DropState.status,color=Muted,modifier=Modifier.padding(top=8.dp,bottom=24.dp))
                            Label("[ 02 / УСТРОЙСТВА ]")
                            if(DropState.peers.isEmpty())Text("Пока никого рядом. Открой HL100 Drop на другом устройстве в той же сети.",color=Muted,modifier=Modifier.padding(vertical=20.dp))
                            DropState.peers.forEach { peer->Column(Modifier.fillMaxWidth().padding(top=16.dp).border(1.dp,Color(0xFF3C3C36)).padding(16.dp)) {
                                Text(peer.name,fontSize=22.sp);Text("${peer.platform.uppercase()} / ${peer.address}",color=Muted,fontSize=12.sp,modifier=Modifier.padding(vertical=12.dp))
                                Action("ОТПРАВИТЬ ФАЙЛЫ →"){selected=peer;documents.launch(arrayOf("*/*"))}
                                TextButton(onClick={selected=peer;sendTree.launch(null)}){Text("ВЫБРАТЬ ПАПКУ")}
                            } }
                            Spacer(Modifier.height(28.dp));Label("[ 03 / ПЕРЕДАЧИ ]")
                            if(DropState.transfers.isEmpty())Text("Твои передачи появятся здесь.",color=Muted,modifier=Modifier.padding(vertical=20.dp))
                            DropState.transfers.forEach { t->Column(Modifier.fillMaxWidth().padding(top=20.dp)) {
                                Text("${if(t.direction=="send")"↑" else "↓"} ${t.name}",fontSize=18.sp)
                                Text(when(t.status){"done"->"Готово ✓";"cancelled"->"Отменено";"error"->"Ошибка: ${t.message}";else->"${bytes(t.done.toDouble())} / ${bytes(t.total.toDouble())}\n${bytes(t.speed)}/с"},color=Muted,modifier=Modifier.padding(vertical=8.dp))
                                LinearProgressIndicator(progress={if(t.status=="done")1f else if(t.total>0)(t.done.toDouble()/t.total).toFloat() else 0f},modifier=Modifier.fillMaxWidth(),color=Red,trackColor=Color(0xFF30302D))
                                if(t.status=="active")TextButton(onClick={lifecycleScope.launch{DropState.cancel(t.id)}}){Text("ОТМЕНИТЬ")}
                            } }
                        }
                        HorizontalDivider(Modifier.padding(top=28.dp,bottom=16.dp),color=Color(0xFF30302D));Text("V ${BuildConfig.VERSION_NAME} / ANDROID\nDIRECT TRANSMISSION / LOCAL NETWORK",color=Muted,fontSize=10.sp,fontFamily=FontFamily.Monospace)
                    }
                }
            }
        }
    }
    @Composable
    private fun SettingsPanel(first:Boolean,onSave:(String,String,Boolean)->Unit,onBack:()->Unit) {
        var name by remember { mutableStateOf(DropState.name.ifBlank{Build.MODEL}) };var folder by remember { mutableStateOf(DropState.folder) };var updates by remember { mutableStateOf(DropState.preferences(this).getBoolean("updates",true)) }
        Label(if(first)"[ 01 / HELLO, FRIEND ]" else "[ SETTINGS ]");Spacer(Modifier.height(24.dp))
        OutlinedTextField(value=name,onValueChange={name=it},label={Text("Имя устройства")},singleLine=true,modifier=Modifier.fillMaxWidth(),shape=RectangleShape)
        Spacer(Modifier.height(20.dp));Text("Папка сохранения",color=Muted);Text(if(folder.isBlank())"Не выбрана" else Uri.parse(folder).lastPathSegment?:folder,fontSize=12.sp,modifier=Modifier.padding(vertical=8.dp))
        Action("ВЫБРАТЬ ПАПКУ"){folderPicked={folder=it.toString()};saveTree.launch(if(folder.isBlank())null else Uri.parse(folder))}
        if(!first){Row(Modifier.padding(vertical=16.dp)){Checkbox(checked=updates,onCheckedChange={updates=it});Text("Проверять обновления при запуске",modifier=Modifier.padding(top=12.dp))};Text("Версия ${BuildConfig.VERSION_NAME}");TextButton(onClick={lifecycleScope.launch{DropState.checkUpdates(true)}}){Text("ПРОВЕРИТЬ ОБНОВЛЕНИЯ")};DropState.release?.let {Text("Версия ${it.version}\n${it.notes}",color=Muted);Action(DropState.updateProgress?:"ОБНОВИТЬСЯ",DropState.updateProgress==null){update()}} }
        if(!first)Text(DropState.status,color=Muted,modifier=Modifier.padding(top=12.dp))
        Spacer(Modifier.height(24.dp));Action(if(first)"НАЧАТЬ →" else "СОХРАНИТЬ",name.isNotBlank()&&folder.isNotBlank()){onSave(name.trim(),folder,updates)}
        if(!first){TextButton(onClick=onBack){Text("НАЗАД")};TextButton(onClick={stopService(Intent(this@MainActivity,TransferService::class.java));DropState.status="Приём выключен"}){Text("ВЫКЛЮЧИТЬ ПРИЁМ")};TextButton(onClick={enableService()}){Text("ВКЛЮЧИТЬ ПРИЁМ")}}
    }
    private fun enableService(){if(Build.VERSION.SDK_INT>=33)notifyPermission.launch(Manifest.permission.POST_NOTIFICATIONS);ContextCompat.startForegroundService(this,Intent(this,TransferService::class.java))}
    private fun update() {
        val release=DropState.release?:return
        if(DropState.busy){DropState.status="Дождитесь завершения передач";return}
        lifecycleScope.launch {try {DropState.updateProgress="Загрузка…";val apk=withContext(Dispatchers.IO){Updates.download(this@MainActivity,release){p->lifecycleScope.launch{DropState.updateProgress="Загрузка $p%"}}};if(DropState.busy){DropState.status="Началась передача. Повторите обновление после завершения";return@launch};pendingApk=apk;installApk()}catch(e:Exception){DropState.status="Не удалось обновиться: ${e.message}"}finally{DropState.updateProgress=null} }
    }
    private fun installApk(){val file=pendingApk?:return;if(!packageManager.canRequestPackageInstalls()){startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:$packageName")));return};pendingApk=null;val uri=FileProvider.getUriForFile(this,"$packageName.files",file);startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri,"application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))}
    override fun onResume(){super.onResume();if(pendingApk!=null&&packageManager.canRequestPackageInstalls())installApk()}
}
@Composable private fun Label(text:String){Text(text,color=Red,fontSize=12.sp,fontFamily=FontFamily.Monospace)}
@Composable private fun Action(text:String,enabled:Boolean=true,onClick:()->Unit){Button(onClick=onClick,enabled=enabled,shape=RectangleShape,contentPadding=PaddingValues(horizontal=18.dp,vertical=14.dp)){Text(text,fontFamily=FontFamily.Monospace)}}
private fun bytes(value:Double):String { var n=value;var i=0;val units=arrayOf("Б","КБ","МБ","ГБ","ТБ");while(n>=1024&&i<units.lastIndex){n/=1024;i++};return "%.1f %s".format(n,units[i]) }
