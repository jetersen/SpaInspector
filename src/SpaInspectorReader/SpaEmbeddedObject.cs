using System.Buffers.Binary;

namespace SpaInspectorReader;

/// <summary>A bounded type-130 object envelope. Selectors identify instances rather than fixed roles.</summary>
public sealed record SpaEmbeddedObject
{
    public int SectionIndex { get; init; }
    public ushort ClassId { get; init; }
    public ushort PayloadClassId { get; init; }
    public ushort? CachedClassId { get; init; }
    public ushort? CachedBodyWord { get; init; }
    public ushort SecondarySelector { get; init; }
    public uint TertiarySelector { get; init; }
    public uint DeclaredBodyLength { get; init; }
    public string Kind { get; init; } = "";
    public byte[] Body { get; init; } = Array.Empty<byte>();
    public byte[] TrailingBytes { get; init; } = Array.Empty<byte>();
    public IReadOnlyList<SpaObjectAttribute> Attributes { get; init; } = Array.Empty<SpaObjectAttribute>();
}

public sealed record SpaObjectAttribute
{
    public ushort Id { get; init; }
    public string? Name { get; init; }
    public uint Offset { get; init; }
    public string Type { get; init; } = "";
    public object? Value { get; init; }
}

internal static class SpaEmbeddedObjectDecoder
{
    public static IReadOnlyList<SpaEmbeddedObject> Decode(IReadOnlyList<SpaSection> sections, SpaTextEncoding encoding, List<string> warnings)
    {
        var result = new List<SpaEmbeddedObject>();
        foreach (var section in sections.Where(section => section.Type == 130))
        {
            var payload = section.Payload;
            if (payload.Length < 6)
            {
                warnings.Add($"Type 130 section {section.Index} has an incomplete object envelope; its raw section is preserved.");
                continue;
            }
            var payloadClass = BinaryPrimitives.ReadUInt16LittleEndian(payload);
            var bodyLength = BinaryPrimitives.ReadUInt32LittleEndian(payload.AsSpan(2, 4));
            if (bodyLength > payload.Length - 6)
            {
                warnings.Add($"Type 130 section {section.Index} declares a body outside its payload; its raw section is preserved.");
                continue;
            }
            var cachedClassWord = (ushort)(section.TertiarySelector >> 16);
            var cachedBodyWord = (ushort)section.TertiarySelector;
            ushort? cachedClass = cachedClassWord == 0 ? null : cachedClassWord;
            var effectiveClass = cachedClass ?? payloadClass;
            var mismatch = cachedClass.HasValue && cachedClass != payloadClass;
            if (mismatch)
                warnings.Add($"Type 130 section {section.Index} cached class {cachedClass} disagrees with payload class {payloadClass}; attributes are not interpreted.");
            var attributes = new List<SpaObjectAttribute>();
            var bodyEnd = checked(6 + (int)bodyLength);
            bool Fits(int offset, int length) => offset >= 6 && offset <= bodyEnd && length <= bodyEnd - offset;
            void AddString(ushort id, string? name, int offset, int length)
            {
                if (!Fits(offset, length)) return;
                var raw = payload.AsSpan(offset, length);
                var terminator = raw.IndexOf((byte)0);
                var content = (terminator < 0 ? raw : raw[..terminator]).ToArray();
                var text = SpaParser.DecodeText(content, encoding, $"Object {section.Index} attribute {id}", warnings);
                attributes.Add(new() { Id = id, Name = name, Offset = (uint)offset, Type = "string", Value = text.Text });
            }
            void AddFloat(ushort id, int offset, string? name = null)
            {
                if (Fits(offset, 4)) attributes.Add(new()
                {
                    Id = id, Name = name, Offset = (uint)offset, Type = "float32",
                    Value = BinaryPrimitives.ReadSingleLittleEndian(payload.AsSpan(offset, 4))
                });
            }
            if (!mismatch)
            {
                switch (effectiveClass)
                {
                    case 121:
                        AddString(2, "shortExperimentPath", 10, 80);
                        AddString(3, "experimentTitle", 90, 64);
                        AddString(4, "description", 154, 255);
                        AddString(7, null, 413, 100);
                        AddString(49, "longExperimentPath", 671, 260);
                        break;
                    case 159:
                        string?[] atrNames = ["applied", "sampleRefractiveIndex", "crystalRefractiveIndex", "numberOfBounces", "angleOfIncidenceDegrees", null, null];
                        for (ushort id = 0; id < 7; id++) AddFloat(id, 6 + 4 * id, atrNames[id]);
                        break;
                    case 166:
                        if (Fits(6, 2)) attributes.Add(new()
                        {
                            Id = 0, Offset = 6, Type = "uint16", Value = BinaryPrimitives.ReadUInt16LittleEndian(payload.AsSpan(6, 2))
                        });
                        AddFloat(1, 8);
                        break;
                }
            }
            result.Add(new SpaEmbeddedObject
            {
                SectionIndex = section.Index, ClassId = effectiveClass, PayloadClassId = payloadClass,
                CachedClassId = cachedClass, CachedBodyWord = cachedBodyWord == 0 ? null : cachedBodyWord,
                SecondarySelector = section.SecondarySelector, TertiarySelector = section.TertiarySelector,
                DeclaredBodyLength = bodyLength,
                Kind = effectiveClass switch { 121 => "experiment", 158 => "instrument_status", 159 => "advanced_atr", 166 => "instrument_specification", _ => $"class_{effectiveClass}" },
                Body = payload.AsSpan(6, (int)bodyLength).ToArray(), TrailingBytes = payload.AsSpan(bodyEnd).ToArray(),
                Attributes = attributes.AsReadOnly()
            });
        }
        return result.AsReadOnly();
    }
}
