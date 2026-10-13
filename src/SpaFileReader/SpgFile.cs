using System.Buffers.Binary;

namespace SpaFileReader;

/// <summary>Legacy terminated-directory group accessor. It does not assert full SPG format compatibility.</summary>
[PublicAPI]
public static class SpgFile
{
    public static IEnumerable<float[]> ReadYUnitAsFloatArray(byte[] bytes)
    {
        var results = new List<float[]>();
        for (var position = 304; ; position += 16)
        {
            if (position > bytes.Length - 16)
                throw new InvalidDataException("Group directory has no complete terminating record.");
            var record = bytes.AsSpan(position, 16);
            var type = BinaryPrimitives.ReadUInt16LittleEndian(record);
            if (type == 0) return results;
            if (type != 3) continue;
            var offset = BinaryPrimitives.ReadUInt32LittleEndian(record.Slice(2, 4));
            var length = BinaryPrimitives.ReadUInt32LittleEndian(record.Slice(6, 4));
            if (offset > bytes.Length || length > (uint)bytes.Length - offset || length % 4 != 0)
                throw new InvalidDataException("Group data section is outside the file or has an incomplete float32 value.");
            var values = new float[length / 4];
            for (var index = 0; index < values.Length; index++)
                values[values.Length - index - 1] = BinaryPrimitives.ReadSingleLittleEndian(bytes.AsSpan((int)offset + index * 4, 4));
            results.Add(values);
        }
    }
    public static IEnumerable<float[]> ReadYUnitAsFloatArray(string file) => ReadYUnitAsFloatArray(File.ReadAllBytes(file));
    public static async Task<IEnumerable<float[]>> ReadYUnitAsFloatArrayAsync(string file) => ReadYUnitAsFloatArray(await File.ReadAllBytesAsync(file));
    public static IEnumerable<double[]> ReadYUnitAsDoubleArray(byte[] bytes) => ReadYUnitAsFloatArray(bytes).Select(values => values.Select(value => (double)value).ToArray()).ToArray();
    public static IEnumerable<double[]> ReadYUnitAsDoubleArray(string file) => ReadYUnitAsDoubleArray(File.ReadAllBytes(file));
    public static async Task<IEnumerable<double[]>> ReadYUnitAsDoubleArrayAsync(string file) => ReadYUnitAsDoubleArray(await File.ReadAllBytesAsync(file));
}
