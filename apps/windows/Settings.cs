using System;
using System.IO;
using System.Text.Json;
namespace HL100.Drop;
internal sealed class Settings {
    public string Id { get; set; } = Guid.NewGuid().ToString("N");
    public string Name { get; set; } = Environment.MachineName;
    public string Folder { get; set; } = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Downloads", "HL100 Drop");
    public bool Configured { get; set; }
    public bool CheckUpdates { get; set; } = true;
    private static string Location => Environment.GetEnvironmentVariable("HL100_SETTINGS_PATH") ?? Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "HL100Drop", "settings.json");
    public static Settings Load() { try { return JsonSerializer.Deserialize<Settings>(File.ReadAllText(Location)) ?? new(); } catch (Exception e) when (e is IOException or JsonException or UnauthorizedAccessException) { return new(); } }
    public void Save() { Directory.CreateDirectory(Path.GetDirectoryName(Location)!); var temp=Location+".tmp";File.WriteAllText(temp,JsonSerializer.Serialize(this));File.Move(temp,Location,true); }
}
