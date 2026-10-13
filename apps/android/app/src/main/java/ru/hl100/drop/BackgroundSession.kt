package ru.hl100.drop

/** Monotonic session deadline; active transfers may outlive reception. */
internal class BackgroundSession {
    var deadline=0L
        private set
    private var expired=false
    fun end(){expired=true;deadline=0}
    fun update(visible:Boolean,enabled:Boolean,minutes:Int,active:Boolean,now:Long):Mode {
        if(visible){deadline=0;expired=false;return Mode.VISIBLE}
        if(enabled&&!expired&&deadline==0L&&minutes>0)deadline=now+minutes*60_000L
        if(deadline>0&&now>=deadline)expired=true
        return if((enabled&&!expired)||active)Mode.BACKGROUND else Mode.STOP
    }
    enum class Mode { VISIBLE, BACKGROUND, STOP }
}
