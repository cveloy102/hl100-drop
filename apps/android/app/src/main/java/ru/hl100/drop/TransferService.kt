package ru.hl100.drop

import android.app.*
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.IBinder
import android.os.SystemClock
import androidx.core.content.IntentCompat
import kotlinx.coroutines.*

/** Owns the engine and URI grants while a transfer outlives the activity. */
class TransferService:Service() {
    companion object {
        private var current:TransferService?=null
        private var visible=false
        private val cleanup=CoroutineScope(SupervisorJob()+Dispatchers.Main)
        fun setVisible(value:Boolean){visible=value;current?.refreshMode()}
        fun start(c:Context){c.startService(Intent(c,TransferService::class.java))}
        fun restart(c:Context){c.startService(Intent(c,TransferService::class.java).setAction("restart"))}
        fun send(c:Context,peer:Peer,uris:List<Uri>,tree:Boolean=false){
            if(uris.isEmpty())return
            val intent=Intent(c,TransferService::class.java).setAction("send")
                .putExtra("peerId",peer.id).putExtra("peerName",peer.name)
                .putExtra("address",peer.address).putExtra("port",peer.port).putExtra("platform",peer.platform)
                .putParcelableArrayListExtra("uris",ArrayList(uris))
                .putExtra("tree",tree)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            intent.clipData=ClipData.newRawUri("DROP",uris.first()).apply{uris.drop(1).forEach{addItem(ClipData.Item(it))}}
            c.startService(intent)
        }
    }
    private val scope=CoroutineScope(SupervisorJob()+Dispatchers.Main)
    private var lock:WifiManager.MulticastLock?=null
    private var foreground=false
    private var pending=0
    private val session=BackgroundSession()
    private var stopped=false
    private var closeJob:Job?=null
    private var notificationKind=""
    override fun onCreate(){
        super.onCreate();current=this
        getSystemService(NotificationManager::class.java).createNotificationChannel(
            NotificationChannel("transfer","Локальный обмен",NotificationManager.IMPORTANCE_LOW))
        lock=(applicationContext.getSystemService(WIFI_SERVICE) as WifiManager)
            .createMulticastLock("hl100-discovery").apply{setReferenceCounted(false);acquire()}
        scope.launch{DropState.start(this@TransferService);while(isActive){refreshMode();delay(1000)}}
    }
    override fun onStartCommand(intent:Intent?,flags:Int,startId:Int):Int {
        if(intent?.action=="stop"){
            // Cancel background reception; ongoing transfers finish first.
            DropState.preferences(this).edit().putBoolean("background",false).apply()
            session.end();DropState.backgroundUntil=0;refreshMode();return START_NOT_STICKY
        }
        if(intent?.action=="restart")scope.launch{DropState.stop();DropState.start(this@TransferService);refreshMode()}
        if(intent?.action=="send"){
            val uris=IntentCompat.getParcelableArrayListExtra(intent,"uris",Uri::class.java).orEmpty()
            val peer=Peer(intent.getStringExtra("peerId").orEmpty(),intent.getStringExtra("peerName").orEmpty(),intent.getStringExtra("address").orEmpty(),intent.getIntExtra("port",41000),intent.getStringExtra("platform").orEmpty())
            pending++;refreshMode()
            scope.launch{try{DropState.start(this@TransferService);DropState.send(this@TransferService,peer,uris,intent.getBooleanExtra("tree",false))}finally{pending--;refreshMode()}}
        }
        refreshMode();return START_NOT_STICKY
    }
    private fun refreshMode(){
        if(stopped)return
        val prefs=DropState.preferences(this)
        val enabled=prefs.getBoolean("background",false)
        val minutes=prefs.getInt("backgroundMinutes",15).coerceAtLeast(0)
        val active=pending>0||DropState.busy
        val mode=session.update(visible,enabled,minutes,active,SystemClock.elapsedRealtime())
        DropState.backgroundUntil=session.deadline
        if(mode==BackgroundSession.Mode.BACKGROUND){
            closeJob?.cancel();closeJob=null
            val kind="$active|$minutes|${session.deadline}"
            if(!foreground||notificationKind!=kind){
                val open=PendingIntent.getActivity(this,0,Intent(this,MainActivity::class.java),PendingIntent.FLAG_IMMUTABLE)
                val stop=PendingIntent.getService(this,1,Intent(this,TransferService::class.java).setAction("stop"),PendingIntent.FLAG_IMMUTABLE)
                val text=if(active)"Передача файлов" else if(minutes==0)"Фоновый приём включён" else "Фоновый приём на $minutes мин"
                val notification=Notification.Builder(this,"transfer").setSmallIcon(android.R.drawable.stat_sys_upload)
                    .setContentTitle("DROP").setContentText(text).setContentIntent(open).setOngoing(true).setOnlyAlertOnce(true)
                    .addAction(Notification.Action.Builder(null,"Выключить приём",stop).build())
                if(session.deadline>0&&!active)notification.setWhen(System.currentTimeMillis()+session.deadline-SystemClock.elapsedRealtime()).setUsesChronometer(true).setChronometerCountDown(true)
                startForeground(100,notification.build())
                foreground=true;notificationKind=kind
            }
        }else{
            if(foreground){stopForeground(STOP_FOREGROUND_REMOVE);foreground=false}
            if(mode==BackgroundSession.Mode.STOP){
                // Let the poll observe a transfer accepted just as the activity leaves.
                if(closeJob==null)closeJob=scope.launch{delay(500);if(!visible&&pending==0&&!DropState.busy){stopped=true;DropState.backgroundUntil=0;stopSelf()};closeJob=null}
            }else{closeJob?.cancel();closeJob=null}
        }
    }
    override fun onDestroy(){
        scope.cancel();if(current===this){current=null;cleanup.launch{if(current==null)DropState.stop();if(visible&&current==null&&DropState.folder.isNotBlank())start(applicationContext)}}
        if(foreground)stopForeground(STOP_FOREGROUND_REMOVE)
        lock?.let{if(it.isHeld)it.release()};super.onDestroy()
    }
    override fun onBind(intent:Intent?):IBinder?=null
}
