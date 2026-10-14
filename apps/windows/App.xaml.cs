using System;
using System.Linq;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Windows;
namespace HL100.Drop;
public partial class App : Application {
    private Mutex? instance;
    private EventWaitHandle? show;
    private RegisteredWaitHandle? listener;
    private bool ownsInstance;
    protected override async void OnStartup(StartupEventArgs e) {
        var identity=Environment.UserName+"|"+(Environment.GetEnvironmentVariable("HL100_SETTINGS_PATH")??"default");
        var name="Local\\HL100Drop-"+Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(identity)))[..24];
        instance=new Mutex(false,name);
        try{ownsInstance=instance.WaitOne(0);}catch(AbandonedMutexException){ownsInstance=true;}
        show=new EventWaitHandle(false,EventResetMode.AutoReset,name+"-show");
        if(!ownsInstance){if(!e.Args.Contains("--login"))show.Set();Shutdown();return;}
        listener=ThreadPool.RegisterWaitForSingleObject(show,(_,_)=>Dispatcher.BeginInvoke(new Action(()=>{if(MainWindow!=null){MainWindow.Show();MainWindow.WindowState=WindowState.Normal;MainWindow.Activate();}})),null,Timeout.Infinite,false);
        base.OnStartup(e);
        var window=new MainWindow();MainWindow=window;
        // Never create a visible HWND for a configured background login.
        // Loaded runs during WPF's implicit Show and is too late to hide safely.
        if(!e.Args.Contains("--login")||!window.IsConfigured)window.Show();
        await window.InitializeAsync();
    }
    protected override void OnExit(ExitEventArgs e) {
        listener?.Unregister(null);show?.Dispose();if(ownsInstance)instance?.ReleaseMutex();instance?.Dispose();base.OnExit(e);
    }
}
