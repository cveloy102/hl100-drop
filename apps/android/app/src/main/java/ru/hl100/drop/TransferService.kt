package ru.hl100.drop

import android.app.*
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.IBinder
import kotlinx.coroutines.*

class TransferService:Service() {
    private val scope=CoroutineScope(SupervisorJob()+Dispatchers.Main)
    private var lock:WifiManager.MulticastLock?=null
    override fun onCreate(){super.onCreate();val manager=getSystemService(NotificationManager::class.java);manager.createNotificationChannel(NotificationChannel("transfer","Локальный обмен",NotificationManager.IMPORTANCE_LOW));val intent=PendingIntent.getActivity(this,0,Intent(this,MainActivity::class.java),PendingIntent.FLAG_IMMUTABLE);startForeground(100,Notification.Builder(this,"transfer").setSmallIcon(android.R.drawable.stat_sys_upload).setContentTitle("HL100 Drop").setContentText("Приём файлов в локальной сети включён").setContentIntent(intent).setOngoing(true).build());lock=(applicationContext.getSystemService(WIFI_SERVICE) as WifiManager).createMulticastLock("hl100-discovery").apply { setReferenceCounted(false);acquire() };scope.launch{DropState.start(this@TransferService)}}
    override fun onStartCommand(intent:Intent?,flags:Int,startId:Int):Int=START_NOT_STICKY
    override fun onDestroy(){scope.cancel();DropState.stop();lock?.let { if(it.isHeld)it.release() };super.onDestroy()}
    override fun onBind(intent:Intent?):IBinder?=null
}
