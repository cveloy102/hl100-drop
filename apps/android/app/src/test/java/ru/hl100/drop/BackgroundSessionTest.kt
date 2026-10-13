package ru.hl100.drop

import org.junit.Assert.assertEquals
import org.junit.Test

class BackgroundSessionTest {
    @Test fun noBackgroundStopsOnlyWhenIdle(){
        val s=BackgroundSession()
        assertEquals(BackgroundSession.Mode.VISIBLE,s.update(true,false,15,false,100))
        assertEquals(BackgroundSession.Mode.BACKGROUND,s.update(false,false,15,true,200))
        assertEquals(BackgroundSession.Mode.STOP,s.update(false,false,15,false,300))
    }
    @Test fun deadlineDoesNotExtendDuringPollingAndTransferCanFinish(){
        val s=BackgroundSession()
        assertEquals(BackgroundSession.Mode.BACKGROUND,s.update(false,true,15,false,100))
        assertEquals(900100L,s.deadline)
        s.update(false,true,15,false,500)
        assertEquals(900100L,s.deadline)
        assertEquals(BackgroundSession.Mode.BACKGROUND,s.update(false,true,15,true,900100))
        assertEquals(BackgroundSession.Mode.STOP,s.update(false,true,15,false,900101))
        s.update(true,true,15,false,900102)
        assertEquals(BackgroundSession.Mode.BACKGROUND,s.update(false,true,15,false,900103))
        assertEquals(1800103L,s.deadline)
    }
    @Test fun unlimitedAndExplicitStop(){
        val s=BackgroundSession()
        assertEquals(BackgroundSession.Mode.BACKGROUND,s.update(false,true,0,false,Long.MAX_VALUE-1))
        assertEquals(0L,s.deadline)
        s.end()
        assertEquals(BackgroundSession.Mode.BACKGROUND,s.update(false,true,0,true,1))
        assertEquals(BackgroundSession.Mode.STOP,s.update(false,true,0,false,2))
    }
}
