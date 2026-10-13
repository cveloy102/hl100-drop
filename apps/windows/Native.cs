using System;
using System.Runtime.InteropServices;
using System.Text.Json;
namespace HL100.Drop;
internal static class Native {
    private static readonly object Gate = new();
    [DllImport("hl100_core", CallingConvention = CallingConvention.Cdecl)] private static extern IntPtr hl100_command([MarshalAs(UnmanagedType.LPUTF8Str)] string command);
    [DllImport("hl100_core", CallingConvention = CallingConvention.Cdecl)] private static extern void hl100_free(IntPtr value);
    public static JsonElement Call(object command) {
        lock (Gate) {
            var ptr = hl100_command(JsonSerializer.Serialize(command));
            try { using var doc = JsonDocument.Parse(Marshal.PtrToStringUTF8(ptr) ?? "{}"); return doc.RootElement.Clone(); }
            finally { hl100_free(ptr); }
        }
    }
    public static void Check(JsonElement result) { if (result.TryGetProperty("ok", out var ok) && !ok.GetBoolean()) throw new InvalidOperationException(result.GetProperty("error").GetString()); }
}
