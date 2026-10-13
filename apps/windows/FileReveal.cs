using System;
using System.IO;
using System.Runtime.InteropServices;

namespace HL100.Drop;
internal static class FileReveal {
    [DllImport("shell32.dll", CharSet=CharSet.Unicode)]
    private static extern IntPtr ILCreateFromPathW(string path);
    [DllImport("shell32.dll")]
    private static extern void ILFree(IntPtr item);
    [DllImport("shell32.dll")]
    private static extern int SHOpenFolderAndSelectItems(IntPtr item,uint count,IntPtr children,uint flags);
    internal static void Show(string path) {
        path=Path.GetFullPath(path);
        if(path.StartsWith(@"\\?\UNC\",StringComparison.OrdinalIgnoreCase))path=@"\\"+path.Substring(8);
        else if(path.StartsWith(@"\\?\",StringComparison.Ordinal))path=path.Substring(4);
        if(!File.Exists(path)&&!Directory.Exists(path))throw new FileNotFoundException("Файл перемещён или удалён",path);
        var item=ILCreateFromPathW(path);
        if(item==IntPtr.Zero)throw new IOException("Не удалось найти файл в Проводнике");
        try { Marshal.ThrowExceptionForHR(SHOpenFolderAndSelectItems(item,0,IntPtr.Zero,0)); }
        finally { ILFree(item); }
    }
}
