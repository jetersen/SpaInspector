using System.Buffers.Binary;
using System.Text;

namespace SpaTestUtils;

/// <summary>Small synthetic containers for structural and adversarial reader tests.</summary>
public static class SpaFixture
{
    public record Section(ushort Type, byte[] Payload, ushort Secondary = 0, uint Tertiary = 0);

    public static byte[] Header(uint mode = 3, uint xCode = 1, uint yCode = 17, float first = 4, float last = 1, uint points = 4)
    {
        var bytes = new byte[140];
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(0, 4), mode);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(4, 4), points);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(8, 4), xCode);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(12, 4), yCode);
        BinaryPrimitives.WriteSingleLittleEndian(bytes.AsSpan(16, 4), first);
        BinaryPrimitives.WriteSingleLittleEndian(bytes.AsSpan(20, 4), last);
        return bytes;
    }

    public static byte[] Floats(params float[] values)
    {
        var bytes = new byte[values.Length * 4];
        for (var index = 0; index < values.Length; index++) BinaryPrimitives.WriteSingleLittleEndian(bytes.AsSpan(index * 4, 4), values[index]);
        return bytes;
    }

    public static byte[] InterferogramHeader(uint count)
    {
        var bytes = new byte[24];
        BinaryPrimitives.WriteUInt32LittleEndian(bytes, count);
        BinaryPrimitives.WriteUInt32LittleEndian(bytes.AsSpan(4, 4), 7);
        return bytes;
    }

    public static byte[] Build(params Section[] sections)
    {
        var units = (sections.Length + 15) / 16;
        var start = 304 + units * 256;
        var bytes = new byte[start + sections.Sum(section => section.Payload.Length)];
        Encoding.ASCII.GetBytes("Spectral Data File\r\n").CopyTo(bytes, 0);
        Encoding.ASCII.GetBytes("Synthetic signal").CopyTo(bytes, 30);
        BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(288, 2), 1);
        BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(290, 2), 970);
        BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(292, 2), (ushort)units);
        BinaryPrimitives.WriteUInt16LittleEndian(bytes.AsSpan(294, 2), (ushort)sections.Length);
        for (var index = 0; index < sections.Length; index++)
        {
            var section = sections[index];
            var entry = bytes.AsSpan(304 + 16 * index, 16);
            BinaryPrimitives.WriteUInt16LittleEndian(entry, section.Type);
            BinaryPrimitives.WriteUInt32LittleEndian(entry.Slice(2, 4), (uint)start);
            BinaryPrimitives.WriteUInt32LittleEndian(entry.Slice(6, 4), (uint)section.Payload.Length);
            BinaryPrimitives.WriteUInt16LittleEndian(entry.Slice(10, 2), section.Secondary);
            BinaryPrimitives.WriteUInt32LittleEndian(entry.Slice(12, 4), section.Tertiary);
            section.Payload.CopyTo(bytes, start);
            start += section.Payload.Length;
        }
        return bytes;
    }

    public static byte[] Spectrum(uint mode = 3, uint xCode = 1, uint yCode = 17, float first = 4, float last = 1, params float[] values)
    {
        if (values.Length == 0) values = [1, 2, 3, 4];
        return Build(new(2, Header(mode, xCode, yCode, first, last, (uint)values.Length)), new(3, Floats(values)));
    }
}
