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
    private sealed class TransferRow {
        public ProgressBar Bar=new(); public TextBlock Text=new(); public Button Cancel=new();
        public long Total; public bool Receiving; public string FirstName=""; public int Count;
        public string? SavedPath; public Dictionary<string,Button> Files=new();
    }
    private readonly Dictionary<string,TransferRow> transfers=new();
    private TextBlock? settingsFeedback;
    private ReceiveToast? toast;
    private readonly System.Windows.Forms.NotifyIcon tray=new();
    private JsonElement[] peers=Array.Empty<JsonElement>();
    private string peerSignature="";
    private bool running, polling, busy, exiting, updating;
    private ReleaseInfo? release;
    public MainWindow() {
        InitializeComponent();
        VersionLabel.Text="V "+Updates.Current+" / WINDOWS";
        tray.Icon=System.Drawing.Icon.ExtractAssociatedIcon(Environment.ProcessPath!)??System.Drawing.SystemIcons.Application;tray.Text="DROP";tray.Visible=true;
        tray.DoubleClick+=(_,_)=>Dispatcher.Invoke(()=>{Show();WindowState=WindowState.Normal;Activate();});
        var menu=new System.Windows.Forms.ContextMenuStrip();menu.Items.Add("Открыть",null,(_,_)=>Dispatcher.Invoke(()=>{Show();Activate();}));menu.Items.Add("Выход",null,(_,_)=>Dispatcher.Invoke(Exit));tray.ContextMenuStrip=menu;
        timer.Tick+=async(_,_)=>await Poll();
        Loaded+=async(_,_)=>{if(!settings.Configured){ShowSettingsPage(true);return;}if(Environment.GetCommandLineArgs().Contains("--login"))Hide();await Start();if(settings.CheckUpdates)await CheckUpdate(false);};
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
            card.Child=stack;card.Drop+=async(_,args)=>{ResetDrop();args.Handled=true;if(args.Data.GetData(DataFormats.FileDrop) is string[] paths)await Send(p,paths);};PeerList.Children.Add(card);
            var ordinaryBorder=card.BorderBrush;var ordinaryBackground=card.Background;
            card.DragOver+=(_,args)=>{if(SettingsPage.Visibility==Visibility.Visible||updating||!args.Data.GetDataPresent(DataFormats.FileDrop))return;card.BorderBrush=(Brush)FindResource("Red");card.Background=new SolidColorBrush(Color.FromRgb(44,24,23));args.Effects=DragDropEffects.Copy;args.Handled=true;DropHintText.Text="Отпустить → "+p.GetProperty("name").GetString();};
            card.DragLeave+=(_,_)=>{card.BorderBrush=ordinaryBorder;card.Background=ordinaryBackground;};

        }
    }
    private async Task Send(JsonElement peer,string[] paths) {
        if(updating){Status.Text="Дождитесь завершения обновления";return;}
        try {Status.Text="Подготовка файлов…";var result=await Task.Run(()=>Native.Call(new{op="send",address=peer.GetProperty("address").GetString(),port=peer.GetProperty("port").GetInt32(),name=peer.GetProperty("name").GetString(),paths}));Native.Check(result);busy=true;Status.Text="Передача началась";}
        catch(Exception e){Status.Text=e.Message;}
    }
    private async void WindowDrop(object sender,DragEventArgs e) {
        ResetDrop();if(SettingsPage.Visibility==Visibility.Visible)return;
        if(e.Handled||e.Data.GetData(DataFormats.FileDrop) is not string[] paths)return;
        if(peers.Length==1){await Send(peers[0],paths);return;}
        Status.Text=peers.Length==0?"Нет доступных получателей":"Перетащи файлы на карточку нужного устройства";
    }
    private void WindowDragOver(object sender,DragEventArgs e) {
        if(SettingsPage.Visibility==Visibility.Visible||updating||peers.Length==0||!e.Data.GetDataPresent(DataFormats.FileDrop)){e.Effects=DragDropEffects.None;return;}
        WindowFrame.BorderBrush=(Brush)FindResource("Red");DropHint.BorderBrush=(Brush)FindResource("Red");DropHint.Background=new SolidColorBrush(Color.FromRgb(44,24,23));
        DropHintText.Text=peers.Length==1?"Отпусти файлы → "+peers[0].GetProperty("name").GetString():"Отпусти файлы на карточку получателя →";e.Effects=DragDropEffects.Copy;
    }
    private void WindowDragLeave(object sender,DragEventArgs e)=>ResetDrop();
    private void WindowQueryContinueDrag(object sender,QueryContinueDragEventArgs e){if(e.EscapePressed||e.Action!=DragAction.Continue)ResetDrop();}
    private void ResetDrop(){foreach(var card in PeerList.Children.OfType<Border>()){card.BorderBrush=new SolidColorBrush(Color.FromRgb(60,60,54));card.Background=new SolidColorBrush(Color.FromRgb(24,24,23));}WindowFrame.BorderBrush=new SolidColorBrush(Color.FromRgb(48,48,45));DropHint.BorderBrush=WindowFrame.BorderBrush;DropHint.Background=Brushes.Transparent;DropHintText.Text="Перетащи файлы или папки\nна карточку получателя →";}
    private void MinimizeWindow(object sender,RoutedEventArgs e)=>WindowState=WindowState.Minimized;
    private void MaximizeWindow(object sender,RoutedEventArgs e)=>WindowState=WindowState==WindowState.Maximized?WindowState.Normal:WindowState.Maximized;
    private void CloseWindow(object sender,RoutedEventArgs e)=>Close();
    private void Reveal(string path){try{if(!File.Exists(path)&&!Directory.Exists(path)){Status.Text="Файл перемещён или удалён";return;}Process.Start(new ProcessStartInfo("explorer.exe",$"/select,\"{path}\""){UseShellExecute=true});}catch(Exception err){Status.Text="Не удалось открыть папку: "+err.Message;}}
    private void ProcessEvent(JsonElement e) {
        var type=e.GetProperty("type").GetString();
        if(type=="error"){Status.Text=e.GetProperty("message").GetString();return;}
        if(!e.TryGetProperty("transfer",out var idValue))return;var id=idValue.GetString()!;
        if(type=="transfer") {
            EmptyTransfers.Visibility=Visibility.Collapsed;var stack=new StackPanel{Margin=new Thickness(0,0,0,24),Tag=id};
            stack.Children.Add(new TextBlock{Text=$"{(e.GetProperty("direction").GetString()=="send"?"↑":"↓")} {e.GetProperty("name").GetString()}",FontSize=18});
            var createdRow=new TransferRow{Total=e.GetProperty("total").GetInt64(),Receiving=e.GetProperty("direction").GetString()=="receive",Count=e.GetProperty("files").GetInt32()};
            createdRow.Text=new TextBlock{Text=$"{createdRow.Count} объектов / {Bytes(createdRow.Total)}",Foreground=(Brush)FindResource("Muted"),Margin=new Thickness(0,8,0,12),TextWrapping=TextWrapping.Wrap};
            createdRow.Cancel=new Button{Content="ОТМЕНИТЬ",Background=Brushes.Transparent,Foreground=(Brush)FindResource("Muted"),HorizontalAlignment=HorizontalAlignment.Left};createdRow.Cancel.Click+=async(_,_)=>await Task.Run(()=>Native.Call(new{op="cancel",transfer=id}));
            if(e.TryGetProperty("entries",out var entries)){foreach(var entry in entries.EnumerateArray()){
                var path=entry.GetProperty("path").GetString()!;if(createdRow.FirstName.Length==0)createdRow.FirstName=path;
                var label=path+" / "+(entry.GetProperty("directory").GetBoolean()?"ПАПКА":Bytes(entry.GetProperty("size").GetDouble()));
                var fileButton=new Button{Content=new TextBlock{Text=label,TextWrapping=TextWrapping.Wrap},IsEnabled=false,Opacity=1,Background=Brushes.Transparent,Foreground=(Brush)FindResource("Paper"),HorizontalContentAlignment=HorizontalAlignment.Left,Padding=new Thickness(0,8,0,8),ToolTip=label};
                createdRow.Files[path]=fileButton;stack.Children.Add(fileButton);
            }if(createdRow.Count>createdRow.Files.Count)stack.Children.Add(new TextBlock{Text=$"Ещё {createdRow.Count-createdRow.Files.Count} объектов",Foreground=(Brush)FindResource("Muted")});}
            stack.Children.Add(createdRow.Text);stack.Children.Add(createdRow.Bar);stack.Children.Add(createdRow.Cancel);TransferList.Children.Insert(0,stack);transfers[id]=createdRow;
            // Keep the UI bounded during long sessions; active transfers are retained.
            TrimHistory();
        }
        if(!transfers.TryGetValue(id,out var row))return;
        if(type=="file"&&e.TryGetProperty("local_path",out var saved)&&saved.ValueKind==JsonValueKind.String){var path=saved.GetString()!;row.SavedPath??=path;if(row.Files.TryGetValue(e.GetProperty("path").GetString()!,out var button)){button.IsEnabled=true;button.ToolTip="Показать в папке: "+path;button.Click+=(_,_)=>Reveal(path);}}
        if(type=="progress"){var done=e.GetProperty("done").GetDouble();var total=e.GetProperty("total").GetDouble();row.Bar.Value=total>0?done/total*100:100;row.Text.Text=$"{Bytes(done)} / {Bytes(total)}\n{Bytes(e.GetProperty("speed").GetDouble())}/с · осталось {TimeSpan.FromSeconds(Math.Min(e.GetProperty("remaining").GetDouble(),864000)):hh\\:mm\\:ss}";}
        if(type=="complete"){var status=e.GetProperty("status").GetString();row.Text.Text=(status=="done"?"Готово ✓":status=="cancelled"?"Отменено":"Ошибка: "+e.GetProperty("message").GetString())+" / "+Bytes(row.Total);row.Cancel.Visibility=Visibility.Collapsed;if(status=="done"){row.Bar.Value=100;Status.Text="Передача завершена";if(row.Receiving&&settings.Notifications){toast?.Close();var savedPath=row.SavedPath??settings.Folder;toast=new ReceiveToast(row.FirstName.Length>0?row.FirstName:"Файлы получены",$"{row.Count} объектов / {Bytes(row.Total)}",()=>Reveal(savedPath));toast.Show();}}transfers.Remove(id);TrimHistory();}
    }
    private void TrimHistory(){while(TransferList.Children.Count>100){var old=TransferList.Children.OfType<StackPanel>().LastOrDefault(s=>s.Tag is string id&&!transfers.ContainsKey(id));if(old==null)break;TransferList.Children.Remove(old);}}
    private static string Bytes(double n){string[] units={"Б","КБ","МБ","ГБ","ТБ"};int i=0;while(n>=1024&&i<units.Length-1){n/=1024;i++;}return $"{n:0.#} {units[i]}";}
    private void OpenSettings(object sender,RoutedEventArgs e) {
        if(SettingsPage.Visibility==Visibility.Visible){if(settings.Configured)CloseSettings();return;}
        ShowSettingsPage(!settings.Configured);
    }
    private void CloseSettings(){SettingsPage.Visibility=Visibility.Collapsed;MainPage.Visibility=Visibility.Visible;ConnectionHeader.Visibility=Visibility.Visible;SettingsButton.Content="НАСТРОЙКИ ↗";}
    private void ShowSettingsPage(bool first) {
        MainPage.Visibility=Visibility.Collapsed;ConnectionHeader.Visibility=Visibility.Collapsed;SettingsPage.Visibility=Visibility.Visible;SettingsPage.ScrollToTop();SettingsButton.Content=first?"ПЕРВОЕ ВКЛЮЧЕНИЕ":"НАЗАД ←";SettingsContent.Children.Clear();
        var stack=SettingsContent;
        stack.Children.Add(new TextBlock{Text=first?"[ HELLO, FRIEND ]":"[ SETTINGS / V "+Updates.Current+" ]",Foreground=(Brush)FindResource("Red"),Margin=new Thickness(0,0,0,20)});
        stack.Children.Add(new TextBlock{Text="Имя устройства"});var name=new TextBox{Text=settings.Name,MinWidth=450,Margin=new Thickness(0,8,0,16)};stack.Children.Add(name);
        stack.Children.Add(new TextBlock{Text="Папка для полученных файлов"});var folder=new TextBox{Text=settings.Folder,Margin=new Thickness(0,8,0,8),IsReadOnly=true,TextWrapping=TextWrapping.Wrap};stack.Children.Add(folder);
        var choose=new Button{Content="ВЫБРАТЬ ПАПКУ",HorizontalAlignment=HorizontalAlignment.Left};choose.Click+=(_,_)=>{var picker=new OpenFolderDialog();if(picker.ShowDialog(this)==true)folder.Text=picker.FolderName;};stack.Children.Add(choose);
        var login=new CheckBox{Content="Запускать DROP при входе в Windows",IsChecked=LoginLaunch.Enabled,Margin=new Thickness(0,24,0,16)};
        var notifications=new CheckBox{Content="Уведомлять о полученных файлах",IsChecked=settings.Notifications,Margin=new Thickness(0,0,0,16)};
        var auto=new CheckBox{Content="Проверять обновления при запуске",IsChecked=settings.CheckUpdates,Margin=new Thickness(0,0,0,20)};
        if(!first){stack.Children.Add(login);stack.Children.Add(notifications);stack.Children.Add(auto);
            var check=new Button{Content=release==null?"ПРОВЕРИТЬ ОБНОВЛЕНИЯ":"ОБНОВИТЬСЯ ДО "+release.Version,HorizontalAlignment=HorizontalAlignment.Left};
            check.Click+=async(_,_)=>{if(release==null){await CheckUpdate(true);if(release!=null)check.Content="ОБНОВИТЬСЯ ДО "+release.Version;}else InstallUpdate(this,new RoutedEventArgs());};stack.Children.Add(check);
        }
        settingsFeedback=new TextBlock{Foreground=(Brush)FindResource("Muted"),TextWrapping=TextWrapping.Wrap,Margin=new Thickness(0,16,0,0)};stack.Children.Add(settingsFeedback);
        var save=new Button{Content=first?"НАЧАТЬ →":"СОХРАНИТЬ",Margin=new Thickness(0,20,0,0),HorizontalAlignment=HorizontalAlignment.Left};
        save.Click+=async(_,_)=>{
            if(string.IsNullOrWhiteSpace(name.Text)||string.IsNullOrWhiteSpace(folder.Text)){settingsFeedback.Text="Укажите имя и папку";return;}
            var restart=!running||settings.Name!=name.Text.Trim()||settings.Folder!=folder.Text;
            if(restart&&busy){settingsFeedback.Text="Имя и папку можно изменить после завершения передач";return;}
            try {
                Directory.CreateDirectory(folder.Text);
                if(!first&&LoginLaunch.Enabled!=(login.IsChecked==true))LoginLaunch.Set(login.IsChecked==true);
                settings.Name=name.Text.Trim();settings.Folder=folder.Text;settings.Configured=true;
                if(!first){settings.CheckUpdates=auto.IsChecked==true;settings.Notifications=notifications.IsChecked==true;}
                settings.Save();if(!settings.Notifications){toast?.Close();toast=null;}
                CloseSettings();if(restart)await Start();if(first&&settings.CheckUpdates)await CheckUpdate(false);
            }catch(Exception err){settingsFeedback.Text="Не удалось сохранить: "+err.Message;}
        };stack.Children.Add(save);
    }
    private async Task CheckUpdate(bool manual) {
        try{release=await Updates.Check();UpdateBanner.Visibility=release==null?Visibility.Collapsed:Visibility.Visible;if(release!=null)UpdateText.Text=$"Доступна новая версия {release.Version}";if(manual&&settingsFeedback!=null)settingsFeedback.Text=release==null?"Установлена актуальная версия":$"Доступна версия {release.Version}\n{release.Notes}";}
        catch(Exception e){if(manual&&settingsFeedback!=null)settingsFeedback.Text="Не удалось проверить обновления: "+e.Message;}
    }
    private async void InstallUpdate(object sender,RoutedEventArgs e) {
        if(release==null||updating)return;if(busy){MessageBox.Show(this,"Сначала дождитесь завершения передач.","Обновление");return;}updating=true;
        try{var path=await Updates.Download(release,new Progress<double>(p=>UpdateText.Text=$"Загрузка обновления: {p:0}%"));var state=await Task.Run(()=>Native.Call(new{op="poll"}));foreach(var item in state.GetProperty("events").EnumerateArray())ProcessEvent(item);if(state.GetProperty("busy").GetBoolean()){MessageBox.Show(this,"Началась входящая передача. Повторите обновление после её завершения.");return;}Process.Start(new ProcessStartInfo(path){UseShellExecute=true});Exit();}
        catch(Exception err){MessageBox.Show(this,"Не удалось обновиться: "+err.Message,"Обновление");}finally{updating=false;if(release!=null)UpdateText.Text=$"Доступна новая версия {release.Version}";}
    }
    private void WindowClosing(object? sender,System.ComponentModel.CancelEventArgs e){if(exiting)return;e.Cancel=true;Hide();}
    private void Exit(){if(busy&&MessageBox.Show(this,"Передачи будут прерваны. Выйти?","HL100 Drop",MessageBoxButton.YesNo)!=MessageBoxResult.Yes)return;exiting=true;timer.Stop();if(running)Native.Call(new{op="stop"});toast?.Close();tray.Dispose();System.Windows.Application.Current.Shutdown();}
}
