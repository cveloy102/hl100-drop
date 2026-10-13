using System;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
namespace HL100.Drop;
internal sealed class ReceiveToast : Window {
    private readonly DispatcherTimer timer=new(){Interval=TimeSpan.FromSeconds(8)};
    public ReceiveToast(string title,string detail,Action open) {
        Title="DROP — файл получен";Width=370;SizeToContent=SizeToContent.Height;WindowStyle=WindowStyle.None;
        ResizeMode=ResizeMode.NoResize;ShowInTaskbar=false;ShowActivated=false;Topmost=true;
        Background=new SolidColorBrush(Color.FromRgb(16,16,16));Foreground=(Brush)Application.Current.FindResource("Paper");
        FontFamily=new FontFamily("Consolas");FontSize=14;
        var content=new StackPanel{Margin=new Thickness(20)};
        var header=new DockPanel();var close=new Button{Content="×",Padding=new Thickness(6,0,6,0),Background=Brushes.Transparent,Foreground=Foreground};
        close.Click+=(_,_)=>Close();DockPanel.SetDock(close,Dock.Right);header.Children.Add(close);
        header.Children.Add(new TextBlock{Text="[ DROP / ПОЛУЧЕНО ]",Foreground=(Brush)Application.Current.FindResource("Red"),FontSize=12});content.Children.Add(header);
        content.Children.Add(new TextBlock{Text=title,FontSize=18,TextWrapping=TextWrapping.Wrap,Margin=new Thickness(0,14,0,8)});
        content.Children.Add(new TextBlock{Text=detail,TextWrapping=TextWrapping.Wrap,Foreground=(Brush)Application.Current.FindResource("Muted")});
        var show=new Button{Content="ПОКАЗАТЬ В ПАПКЕ ↗",Margin=new Thickness(0,16,0,0),HorizontalAlignment=HorizontalAlignment.Left};show.Click+=(_,_)=>{open();Close();};content.Children.Add(show);
        Content=new Border{BorderBrush=(Brush)Application.Current.FindResource("Red"),BorderThickness=new Thickness(1),Child=content};
        Loaded+=(_,_)=>{var area=SystemParameters.WorkArea;Left=area.Right-ActualWidth-20;Top=area.Bottom-ActualHeight-20;timer.Start();};
        timer.Tick+=(_,_)=>Close();Closed+=(_,_)=>timer.Stop();
    }
}
