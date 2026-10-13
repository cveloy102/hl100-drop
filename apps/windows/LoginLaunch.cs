using System;
using Microsoft.Win32;
namespace HL100.Drop;
internal static class LoginLaunch {
    private const string Key = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string Name = "HL100Drop";
    public static bool Enabled {
        get { using var key=Registry.CurrentUser.OpenSubKey(Key); return key?.GetValue(Name) is string; }
    }
    public static void Set(bool enabled) {
        using var key=Registry.CurrentUser.CreateSubKey(Key);
        if(enabled)key.SetValue(Name,$"\"{Environment.ProcessPath}\" --login");
        else key.DeleteValue(Name,false);
    }
}
