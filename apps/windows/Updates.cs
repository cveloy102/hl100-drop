using System;
using System.IO;
using System.Linq;
using System.Net.Http;
using System.Security.Cryptography;
using System.Text.Json;
using System.Threading.Tasks;
namespace HL100.Drop;
internal sealed record ReleaseInfo(string Version, string Notes, string Url, string Digest, string? Checksums);
internal static class Updates {
    public static string Current => typeof(Updates).Assembly.GetName().Version?.ToString(3) ?? "0.1.0";
    public const string Repository = "cveloy102/hl100-drop";
    private static readonly HttpClient Http = CreateClient();
    private static HttpClient CreateClient() { var c=new HttpClient { Timeout=TimeSpan.FromMinutes(20) };c.DefaultRequestHeaders.UserAgent.ParseAdd("HL100Drop/"+Current);return c; }
    public static async Task<ReleaseInfo?> Check() {
        using var response=await Http.GetAsync($"https://api.github.com/repos/{Repository}/releases/latest");
        if (response.StatusCode==System.Net.HttpStatusCode.NotFound) return null;
        response.EnsureSuccessStatusCode();using var doc=JsonDocument.Parse(await response.Content.ReadAsStringAsync());var root=doc.RootElement;
        var version=root.GetProperty("tag_name").GetString()!.TrimStart('v');
        if (!Version.TryParse(version,out var latest) || latest<=Version.Parse(Current)) return null;
        var assets=root.GetProperty("assets").EnumerateArray().ToArray();
        var asset=assets.FirstOrDefault(a=>a.GetProperty("name").GetString()==$"HL100DropSetup-{version}.exe");
        if (asset.ValueKind==JsonValueKind.Undefined) return null;
        var checksums=assets.FirstOrDefault(a=>a.GetProperty("name").GetString()=="checksums.txt");
        return new(version,root.GetProperty("body").GetString()??"",asset.GetProperty("browser_download_url").GetString()!,asset.TryGetProperty("digest",out var d)?d.GetString()??"":"",checksums.ValueKind==JsonValueKind.Undefined?null:checksums.GetProperty("browser_download_url").GetString());
    }
    public static async Task<string> Download(ReleaseInfo release, IProgress<double> progress) {
        ValidateUrl(release.Url);
        var dir=Path.Combine(Path.GetTempPath(),"HL100Drop",Guid.NewGuid().ToString("N"));Directory.CreateDirectory(dir);
        var path=Path.Combine(dir,$"HL100DropSetup-{release.Version}.exe");
        var digest=release.Digest.StartsWith("sha256:")?release.Digest[7..]:"";
        if (digest.Length!=64 && release.Checksums!=null) {
            ValidateUrl(release.Checksums);var lines=(await Http.GetStringAsync(release.Checksums)).Split('\n');
            digest=lines.Select(l=>l.Trim().Split(' ',StringSplitOptions.RemoveEmptyEntries)).Where(p=>p.Length==2 && p[1].TrimStart('*')==Path.GetFileName(path)).Select(p=>p[0]).FirstOrDefault()??"";
        }
        if (digest.Length!=64) throw new InvalidOperationException("В релизе отсутствует контрольная сумма установщика");
        try {
            using var response=await Http.GetAsync(release.Url,HttpCompletionOption.ResponseHeadersRead);response.EnsureSuccessStatusCode();
            var total=response.Content.Headers.ContentLength??0;await using var input=await response.Content.ReadAsStreamAsync();
            await using (var output=File.Create(path)) {var buffer=new byte[1024*1024];long done=0;int n;while((n=await input.ReadAsync(buffer))>0){await output.WriteAsync(buffer.AsMemory(0,n));done+=n;progress.Report(total>0?done*100.0/total:0);}}
            await using var file=File.OpenRead(path);var hash=Convert.ToHexString(await SHA256.HashDataAsync(file));
            if (!hash.Equals(digest,StringComparison.OrdinalIgnoreCase)) throw new InvalidOperationException("Контрольная сумма обновления не совпала");
            return path;
        } catch {File.Delete(path);throw;}
    }
    private static void ValidateUrl(string url) {var uri=new Uri(url);if(uri.Scheme!="https" || uri.Host!="github.com" || !uri.AbsolutePath.StartsWith("/"+Repository+"/releases/download/",StringComparison.Ordinal))throw new InvalidOperationException("Некорректный адрес обновления");}
}
