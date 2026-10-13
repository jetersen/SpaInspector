using System.Text;
using System.Text.Json;

namespace SpaInspectorReader.Tests;

/// <summary>Test-only export hook for tools/check-parity.py; no C# application is shipped.</summary>
public class ParityExportTests
{
    [ParityExportFact]
    public void ExportConfiguredFixturesForRustComparison()
    {
        var manifestPath = Environment.GetEnvironmentVariable("SPA_PARITY_MANIFEST")!;
        using var manifest = JsonDocument.Parse(File.ReadAllText(manifestPath));
        Assert.NotEmpty(manifest.RootElement.EnumerateArray());
        foreach (var item in manifest.RootElement.EnumerateArray())
        {
            var bytes = File.ReadAllBytes(item.GetProperty("input").GetString()!);
            var original = bytes.ToArray();
            var document = SpaParser.Parse(bytes);
            Export(item.GetProperty("json").GetString()!, document.ToJson());
            Export(item.GetProperty("csv").GetString()!, document.ToCsv());
            Assert.Equal(original, bytes);
            Assert.Equal(original, document.SourceBytes);
        }
    }

    private static void Export(string path, string text)
    {
        using var stream = new FileStream(path, FileMode.CreateNew, FileAccess.Write, FileShare.None);
        using var writer = new StreamWriter(stream, new UTF8Encoding(false));
        writer.Write(text);
    }
}

public sealed class ParityExportFactAttribute : FactAttribute
{
    public ParityExportFactAttribute()
    {
        if (string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("SPA_PARITY_MANIFEST")))
            Skip = "tools/check-parity.py configures the export manifest for the cross-reader comparison.";
    }
}
