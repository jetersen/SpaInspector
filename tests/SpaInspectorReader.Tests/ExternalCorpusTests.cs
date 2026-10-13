using System.Text.Json;

namespace SpaInspectorReader.Tests;

/// <summary>Opt-in conformance against privately held files; no external corpus is bundled.</summary>
public class ExternalCorpusTests
{
    [ExternalCorpusFact]
    public void AllSpaFilesInConfiguredCorpusHaveCoherentSeriesAndLosslessExports()
    {
        var root = Environment.GetEnvironmentVariable("SPA_CORPUS_DIR")!;
        var files = Directory.EnumerateFiles(root, "*", SearchOption.AllDirectories)
            .Where(file => string.Equals(Path.GetExtension(file), ".spa", StringComparison.OrdinalIgnoreCase)).ToArray();
        Assert.NotEmpty(files);
        foreach (var file in files)
        {
            var bytes = File.ReadAllBytes(file);
            var original = bytes.ToArray();
            var spa = SpaParser.Parse(bytes);
            Assert.Equal(original, bytes);
            Assert.Equal(original, spa.SourceBytes);
            Assert.NotEmpty(spa.Series);
            foreach (var series in spa.Series)
            {
                Assert.Equal(series.Metadata.PointCount, (uint)series.Y.Length);
                Assert.Equal(series.Y.Length, series.X.Length);
                if (series.X.Length > 0) Assert.Equal((double)series.Metadata.FirstX, series.X[0]);
                if (series.X.Length > 1) Assert.Equal((double)series.Metadata.LastX, series.X[^1]);
            }
            using var json = JsonDocument.Parse(spa.ToJson());
            Assert.Equal(original, json.RootElement.GetProperty("sourceBytes").GetBytesFromBase64());
            Assert.Equal(spa.Sections.Count, json.RootElement.GetProperty("sections").GetArrayLength());
            Assert.Equal(spa.Series.Sum(series => series.Y.Length) + 1, spa.ToCsv().Count(value => value == '\n'));
        }
    }
}

public sealed class ExternalCorpusFactAttribute : FactAttribute
{
    public ExternalCorpusFactAttribute()
    {
        if (string.IsNullOrWhiteSpace(Environment.GetEnvironmentVariable("SPA_CORPUS_DIR")))
            Skip = "Set SPA_CORPUS_DIR to an external directory containing SPA fixtures.";
    }
}
