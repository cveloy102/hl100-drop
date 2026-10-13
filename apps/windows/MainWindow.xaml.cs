using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
using Microsoft.Win32;
namespace HL100.Drop;
public partial class MainWindow : Window {
    private Settings settings=Settings.Load();
    private readonly DispatcherTimer timer=new() { Interval=TimeSpan.FromMilliseconds(200) };
    private readonly Dictionary<string,(ProgressBar Bar,TextBlock Text,Button Cancel)> transfers=new();
    private readonly System.Windows.Forms.NotifyIcon tray=new();
    private JsonElement[] peers=Array.Empty<JsonElement>();
    private string peerSignature="";
    private bool running, polling, busy, exiting, updating;
    private ReleaseInfo? release;
    public MainWindow() {
        InitializeComponent();
        VersionLabel.Text="V "+Updates.Current+" / WINDOWS";
        tray.Icon=System.Drawing.Icon.ExtractAssociatedIcon(Environment.ProcessPath!)??System.Drawing.SystemIcons.Application;tray.Text="HL100 Drop";tray.Visible=true;
        tray.DoubleClick+=(_,_)=>Dispatcher.Invoke(()=>{Show();WindowState=WindowState.Normal;Activate();});
        var menu=new System.Windows.Forms.ContextMenuStrip();menu.Items.Add("Открыть",null,(_,_)=>Dispatcher.Invoke(()=>{Show();Activate();}));menu.Items.Add("Выход",null,(_,_)=>Dispatcher.Invoke(Exit));tray.ContextMenuStrip=menu;
        timer.Tick+=async(_,_)=>await Poll();
        Loaded+=async(_,_)=>{if(!settings.Configured){if(!ShowSettings(true)){Exit();return;}}await Start();if(settings.CheckUpdates)await CheckUpdate(false);};
    }
    private async Task Start() {
        try {var result=await Task.Run(()=>Native.Call(new{op="start",config=new{id=settings.Id,name=settings.Name,folder=settings.Folder}}));Native.Check(result);running=true;Identity.Text=$"{settings.Name} / ПРИЁМ: {settings.Folder}";Status.Text="Готов к отправке и автоматическому приёму";timer.Start();}
        catch(Exception e){Status.Text="Не удалось запустить: "+e.Message;}
    }
    private async Task Poll() {
        if(!running||polling)return;polling=true;
        try {
            var result=await Task.Run(()=>Native.Call(new{op="poll"}));Native.Check(result);busy=result.GetProperty("busy").GetBoolean();
            peers=result.GetProperty("peers").EnumerateArray().Select(p=>p.Clone()).OrderBy(p=>p.GetProperty("name").GetString()).ToArray();
            var signature=string.Join("|",peers.Select(p=>p.ToString()));if(signature!=peerSignature){peerSignature=signature;DrawPeers();}
            foreach(var e in result.GetProperty("events").EnumerateArray())ProcessEvent(e);
        }catch(Exception e){Status.Text=e.Message;}finally{polling=false;}
    }
    private void DrawPeers() {
        PeerList.Children.Clear();EmptyPeers.Visibility=peers.Length==0?Visibility.Visible:Visibility.Collapsed;
        foreach(var p in peers) {
            var card=new Border{BorderBrush=new SolidColorBrush(Color.FromRgb(60,60,54)),BorderThickness=new Thickness(1),Padding=new Thickness(18),Margin=new Thickness(0,0,0,12),Background=new SolidColorBrush(Color.FromRgb(24,24,23)),AllowDrop=true};
            var stack=new StackPanel();stack.Children.Add(new TextBlock{Text=p.GetProperty("name").GetString(),FontSize=20});stack.Children.Add(new TextBlock{Text=$"{p.GetProperty("platform").GetString()?.ToUpperInvariant()} / {p.GetProperty("address").GetString()}",Foreground=(Brush)FindResource("Muted"),Margin=new Thickness(0,8,0,12)});
            var files=new Button{Content="ОТПРАВИТЬ ФАЙЛЫ →",HorizontalAlignment=HorizontalAlignment.Left};files.Click+=async(_,_)=>{var dialog=new OpenFileDialog{Multiselect=true};if(dialog.ShowDialog(this)==true)await Send(p,dialog.FileNames);};stack.Children.Add(files);
            var folder=new Button{Content="ВЫБРАТЬ ПАПКУ",Background=Brushes.Transparent,Foreground=(Brush)FindResource("Paper"),HorizontalAlignment=HorizontalAlignment.Left,Margin=new Thickness(0,8,0,0)};folder.Click+=async(_,_)=>{var dialog=new OpenFolderDialog();if(dialog.ShowDialog(this)==true)await Send(p,new[]{dialog.FolderName});};stack.Children.Add(folder);
            card.Child=stack;card.Drop+=async(_,args)=>{args.Handled=true;if(args.Data.GetData(DataFormats.FileDrop) is string[] paths)await Send(p,paths);};PeerList.Children.Add(card);
        }
    }
    private async Task Send(JsonElement peer,string[] paths) {
        if(updating){Status.Text="Дождитесь завершения обновления";return;}
        try {Status.Text="Подготовка файлов…";var result=await Task.Run(()=>Native.Call(new{op="send",address=peer.GetProperty("address").GetString(),port=peer.GetProperty("port").GetInt32(),name=peer.GetProperty("name").GetString(),paths}));Native.Check(result);busy=true;Status.Text="Передача началась";}
        catch(Exception e){Status.Text=e.Message;}
    }
    private async void WindowDrop(object sender,DragEventArgs e) {
        if(e.Handled||e.Data.GetData(DataFormats.FileDrop) is not string[] paths)return;
        if(peers.Length==1){await Send(peers[0],paths);return;}
        Status.Text=peers.Length==0?"Нет доступных получателей":"Перетащи файлы на карточку нужного устройства";
    }
    private void ProcessEvent(JsonElement e) {
        var type=e.GetProperty("type").GetString();
        if(type=="error"){Status.Text=e.GetProperty("message").GetString();return;}
        if(!e.TryGetProperty("transfer",out var idValue))return;var id=idValue.GetString()!;
        if(type=="transfer") {
            EmptyTransfers.Visibility=Visibility.Collapsed;var stack=new StackPanel{Margin=new Thickness(0,0,0,24)};
            stack.Children.Add(new TextBlock{Text=$"{(e.GetProperty("direction").GetString()=="send"?"↑":"↓")} {e.GetProperty("name").GetString()}",FontSize=18});
            var text=new TextBlock{Text=$"{e.GetProperty("files").GetInt32()} объектов / {Bytes(e.GetProperty("total").GetDouble())}",Foreground=(Brush)FindResource("Muted"),Margin=new Thickness(0,8,0,12),TextWrapping=TextWrapping.Wrap};var bar=new ProgressBar();var cancel=new Button{Content="ОТМЕНИТЬ",Background=Brushes.Transparent,Foreground=(Brush)FindResource("Muted"),HorizontalAlignment=HorizontalAlignment.Left};cancel.Click+=async(_,_)=>await Task.Run(()=>Native.Call(new{op="cancel",transfer=id}));stack.Children.Add(text);stack.Children.Add(bar);stack.Children.Add(cancel);TransferList.Children.Insert(0,stack);transfers[id]=(bar,text,cancel);
            // Keep the UI bounded during long sessions; active transfers are retained.
            if(TransferList.Children.Count>100){var last=TransferList.Children[TransferList.Children.Count-1];if(last is StackPanel s&&s.Children.OfType<Button>().All(b=>b.Visibility==Visibility.Collapsed))TransferList.Children.Remove(last);}
        }
        if(!transfers.TryGetValue(id,out var row))return;
        if(type=="progress"){var done=e.GetProperty("done").GetDouble();var total=e.GetProperty("total").GetDouble();row.Bar.Value=total>0?done/total*100:100;row.Text.Text=$"{Bytes(done)} / {Bytes(total)}\n{Bytes(e.GetProperty("speed").GetDouble())}/с · осталось {TimeSpan.FromSeconds(Math.Min(e.GetProperty("remaining").GetDouble(),864000)):hh\\:mm\\:ss}";}
        if(type=="complete"){var status=e.GetProperty("status").GetString();row.Text.Text=status=="done"?"Готово ✓":status=="cancelled"?"Отменено":"Ошибка: "+e.GetProperty("message").GetString();row.Cancel.Visibility=Visibility.Collapsed;if(status=="done")row.Bar.Value=100;transfers.Remove(id);}
    }
    private static string Bytes(double n){string[] units={"Б","КБ","МБ","ГБ","ТБ"};int i=0;while(n>=1024&&i<units.Length-1){n/=1024;i++;}return $"{n:0.#} {units[i]}";}
    private async void OpenSettings(object sender,RoutedEventArgs e){if(busy){MessageBox.Show(this,"Настройки можно изменить после завершения передач.","HL100 Drop");return;}if(ShowSettings(false))await Start();}
    private bool ShowSettings(bool first) {
        var dialog=new Window{Title=first?"Hello, friend — первое включение":"Настройки HL100 Drop",Width=580,Height=first?390:520,Owner=this,WindowStartupLocation=WindowStartupLocation.CenterOwner,ResizeMode=ResizeMode.NoResize};var stack=new StackPanel{Margin=new Thickness(28)};
        stack.Children.Add(new TextBlock{Text=first?"[ 01 / HELLO, FRIEND ]":"[ SETTINGS / V "+Updates.Current+" ]",Foreground=(Brush)FindResource("Red"),Margin=new Thickness(0,0,0,20)});
        stack.Children.Add(new TextBlock{Text="Имя устройства"});var name=new TextBox{Text=settings.Name,Margin=new Thickness(0,8,0,16)};stack.Children.Add(name);
        stack.Children.Add(new TextBlock{Text="Папка для полученных файлов"});var folder=new TextBox{Text=settings.Folder,Margin=new Thickness(0,8,0,8),IsReadOnly=true};stack.Children.Add(folder);var choose=new Button{Content="ВЫБРАТЬ ПАПКУ",HorizontalAlignment=HorizontalAlignment.Left};choose.Click+=(_,_)=>{var picker=new OpenFolderDialog();if(picker.ShowDialog(dialog)==true)folder.Text=picker.FolderName;};stack.Children.Add(choose);
        var auto=new CheckBox{Content="Проверять обновления при запуске",IsChecked=settings.CheckUpdates,Margin=new Thickness(0,16,0,8)};if(!first){stack.Children.Add(auto);var check=new Button{Content=release==null?"ПРОВЕРИТЬ ОБНОВЛЕНИЯ":"ОБНОВИТЬСЯ ДО "+release.Version,HorizontalAlignment=HorizontalAlignment.Left};check.Click+=async(_,_)=>{if(release==null){await CheckUpdate(true);if(release!=null)check.Content="ОБНОВИТЬСЯ ДО "+release.Version;}else{dialog.Close();InstallUpdate(this,new RoutedEventArgs());}};stack.Children.Add(check);}
        var save=new Button{Content=first?"НАЧАТЬ →":"СОХРАНИТЬ",Margin=new Thickness(0,20,0,0)};save.Click+=(_,_)=>{if(string.IsNullOrWhiteSpace(name.Text)||string.IsNullOrWhiteSpace(folder.Text)){MessageBox.Show(dialog,"Укажите имя и папку");return;}try{Directory.CreateDirectory(folder.Text);settings.Name=name.Text.Trim();settings.Folder=folder.Text;settings.Configured=true;settings.CheckUpdates=auto.IsChecked==true;settings.Save();dialog.DialogResult=true;}catch(Exception err){MessageBox.Show(dialog,err.Message);}};stack.Children.Add(save);dialog.Content=stack;return dialog.ShowDialog()==true;
    }
    private async Task CheckUpdate(bool manual) {
        try{release=await Updates.Check();UpdateBanner.Visibility=release==null?Visibility.Collapsed:Visibility.Visible;if(release!=null)UpdateText.Text=$"Доступна новая версия {release.Version}";if(manual)MessageBox.Show(this,release==null?"Установлена актуальная версия":$"Доступна версия {release.Version}\n\n{release.Notes}","Обновления");}
        catch(Exception e){if(manual)MessageBox.Show(this,"Не удалось проверить обновления: "+e.Message,"Обновления");}
    }
    private async void InstallUpdate(object sender,RoutedEventArgs e) {
        if(release==null||updating)return;if(busy){MessageBox.Show(this,"Сначала дождитесь завершения передач.","Обновление");return;}updating=true;
        try{var path=await Updates.Download(release,new Progress<double>(p=>UpdateText.Text=$"Загрузка обновления: {p:0}%"));var state=await Task.Run(()=>Native.Call(new{op="poll"}));if(state.GetProperty("busy").GetBoolean()){MessageBox.Show(this,"Началась входящая передача. Повторите обновление после её завершения.");return;}Process.Start(new ProcessStartInfo(path){UseShellExecute=true});Exit();}
        catch(Exception err){MessageBox.Show(this,"Не удалось обновиться: "+err.Message,"Обновление");}finally{updating=false;if(release!=null)UpdateText.Text=$"Доступна новая версия {release.Version}";}
    }
    private void WindowClosing(object? sender,System.ComponentModel.CancelEventArgs e){if(exiting)return;e.Cancel=true;Hide();}
    private void Exit(){if(busy&&MessageBox.Show(this,"Передачи будут прерваны. Выйти?","HL100 Drop",MessageBoxButton.YesNo)!=MessageBoxResult.Yes)return;exiting=true;timer.Stop();if(running)Native.Call(new{op="stop"});tray.Dispose();System.Windows.Application.Current.Shutdown();}
}
