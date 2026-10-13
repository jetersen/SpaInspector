using System.Buffers.Binary;
using System.Text;
using System.Text.Json.Serialization;

namespace SpaInspectorReader;

/// <summary>A counted OMNIC directory record. Types and selectors are independent fields.</summary>
public sealed record SpaSection
{
    [JsonIgnore] public int Index { get; init; }
    public ushort Type { get; init; }
    public uint Offset { get; init; }
    public uint Length { get; init; }
    public ushort SecondarySelector { get; init; }
    public uint TertiarySelector { get; init; }
    [JsonIgnore] public ushort Secondary => SecondarySelector;
    [JsonIgnore] public uint Tertiary => TertiarySelector;
    public byte[] Payload { get; init; } = Array.Empty<byte>();
}

/// <summary>Validates and preserves the fixed header and every counted section.</summary>
public sealed class SpaDirectory
{
    public const int FixedHeaderLength = 304;
    public byte[] OriginalBytes { get; private init; } = Array.Empty<byte>();
    public ushort RevisionCandidate { get; private init; }
    public ushort FileTypeCode { get; private init; }
    public ushort DirectoryAllocationUnits { get; private init; }
    public uint RawTimestamp { get; private init; }
    public IReadOnlyList<SpaSection> Entries { get; private init; } = Array.Empty<SpaSection>();

    public SpaSection? Find(ushort type, ushort secondary = 0, uint tertiary = 0) =>
        Entries.FirstOrDefault(entry => entry.Type == type && entry.Secondary == secondary && entry.Tertiary == tertiary);

    public static SpaDirectory Parse(ReadOnlySpan<byte> bytes)
    {
        if (bytes.Length < FixedHeaderLength)
            throw new InvalidDataException("SPA file is shorter than its 304-byte fixed header.");
        if (!bytes[..21].SequenceEqual(Encoding.ASCII.GetBytes("Spectral Data File\r\n\0")))
            throw new InvalidDataException("Invalid SPA signature; expected Spectral Data File followed by CRLF and NUL.");

        var count = BinaryPrimitives.ReadUInt16LittleEndian(bytes.Slice(294, 2));
        if (count > (bytes.Length - FixedHeaderLength) / 16)
            throw new InvalidDataException("SPA directory count exceeds the available file bytes.");
        var allocation = BinaryPrimitives.ReadUInt16LittleEndian(bytes.Slice(292, 2));
        if (allocation > 10000 || FixedHeaderLength + allocation * 256 > bytes.Length)
            throw new InvalidDataException("SPA reserved directory allocation extends outside the file or exceeds its supported bound.");
        if (count > allocation * 16)
            throw new InvalidDataException("SPA directory count exceeds its declared allocation.");

        var original = bytes.ToArray();
        var entries = new List<SpaSection>(count);
        for (var index = 0; index < count; index++)
        {
            var record = bytes.Slice(FixedHeaderLength + index * 16, 16);
            var offset = BinaryPrimitives.ReadUInt32LittleEndian(record.Slice(2, 4));
            var length = BinaryPrimitives.ReadUInt32LittleEndian(record.Slice(6, 4));
            if (offset > bytes.Length || length > (uint)bytes.Length - offset)
                throw new InvalidDataException($"SPA section {index} extends outside the file.");
            if (length != 0 && offset < FixedHeaderLength + allocation * 256)
                throw new InvalidDataException($"SPA section {index} overlaps the fixed header or reserved directory.");
            entries.Add(new SpaSection
            {
                Index = index,
                Type = BinaryPrimitives.ReadUInt16LittleEndian(record),
                Offset = offset,
                Length = length,
                SecondarySelector = BinaryPrimitives.ReadUInt16LittleEndian(record.Slice(10, 2)),
                TertiarySelector = BinaryPrimitives.ReadUInt32LittleEndian(record.Slice(12, 4)),
                Payload = bytes.Slice((int)offset, (int)length).ToArray()
            });
        }

        return new SpaDirectory
        {
            OriginalBytes = original,
            FileTypeCode = BinaryPrimitives.ReadUInt16LittleEndian(bytes.Slice(288, 2)),
            RevisionCandidate = BinaryPrimitives.ReadUInt16LittleEndian(bytes.Slice(290, 2)),
            DirectoryAllocationUnits = allocation,
            RawTimestamp = BinaryPrimitives.ReadUInt32LittleEndian(bytes.Slice(296, 4)),
            Entries = entries.AsReadOnly()
        };
    }

    public static float[] ReadFloat32(SpaSection section)
    {
        if (section.Payload.Length % 4 != 0)
            throw new InvalidDataException($"SPA section {section.Index} has an incomplete float32 value.");
        var result = new float[section.Payload.Length / 4];
        for (var index = 0; index < result.Length; index++)
            result[index] = BinaryPrimitives.ReadSingleLittleEndian(section.Payload.AsSpan(index * 4, 4));
        return result;
    }
}
