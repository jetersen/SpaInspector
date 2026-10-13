using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace SpaInspectorReader;

public static class SpaExport
{
    public static string ToJson(Spa document) => JsonSerializer.Serialize(document, SpaJsonContext.Default.Spa);

    public static string ToCsv(Spa document)
    {
        var output = new StringBuilder("series,secondary_selector,tertiary_selector,index,x,y,x_unit_code,y_unit_code\n");
        foreach (var series in document.Series)
        {
            if (series.X.Length != series.Y.Length)
                throw new InvalidDataException("Cannot export a signal with unequal X and Y counts.");
            for (var index = 0; index < series.Y.Length; index++)
            {
                output.Append(CsvField(series.Kind)).Append(',')
                    .Append(series.SecondarySelector.ToString(CultureInfo.InvariantCulture)).Append(',')
                    .Append(series.TertiarySelector.ToString(CultureInfo.InvariantCulture)).Append(',')
                    .Append(index.ToString(CultureInfo.InvariantCulture)).Append(',')
                    .Append(series.X[index].ToString("R", CultureInfo.InvariantCulture)).Append(',')
                    .Append(series.Y[index].ToString("R", CultureInfo.InvariantCulture)).Append(',')
                    .Append(series.Metadata.XUnitCode.ToString(CultureInfo.InvariantCulture)).Append(',')
                    .Append(series.Metadata.YUnitCode.ToString(CultureInfo.InvariantCulture)).Append('\n');
            }
        }
        return output.ToString();
    }

    private static string CsvField(string value) => value.IndexOfAny([',', '"', '\r', '\n']) < 0 ? value : '"' + value.Replace("\"", "\"\"") + '"';
}

[JsonSourceGenerationOptions(
    PropertyNamingPolicy = JsonKnownNamingPolicy.CamelCase,
    NumberHandling = JsonNumberHandling.AllowNamedFloatingPointLiterals,
    WriteIndented = true,
    GenerationMode = JsonSourceGenerationMode.Metadata)]
[JsonSerializable(typeof(Spa))]
[JsonSerializable(typeof(float))]
[JsonSerializable(typeof(ushort))]
[JsonSerializable(typeof(string))]
internal partial class SpaJsonContext : JsonSerializerContext;
