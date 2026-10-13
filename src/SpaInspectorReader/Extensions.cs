namespace SpaInspectorReader;

public static class Extensions
{
    public static Spa ReadSpa(this Stream stream, SpaReadOptions? options = null, bool leaveOpen = true)
    {
        using var reader = new Read(stream, leaveOpen, options);
        return reader.ReadBinaryToSpa();
    }

    public static void Position(this BinaryReader binaryReader, long position) => binaryReader.BaseStream.Position = position;

    /// <summary>Reads a bounded byte-terminated field using the SPA legacy text policy.</summary>
    public static string ReadNullTerminatedString(this BinaryReader reader, int maximumBytes = 256, SpaTextEncoding encoding = SpaTextEncoding.Auto)
    {
        if (maximumBytes < 0) throw new ArgumentOutOfRangeException(nameof(maximumBytes));
        var bytes = new List<byte>();
        for (var index = 0; index < maximumBytes; index++)
        {
            byte value;
            try { value = reader.ReadByte(); }
            catch (EndOfStreamException) { break; }
            if (value == 0) break;
            bytes.Add(value);
        }
        return SpaParser.DecodeText(bytes.ToArray(), encoding, "Text", new List<string>()).Text;
    }

    public static async Task<MemoryStream> FileAsMemoryStream(this string file)
    {
        var memoryStream = new MemoryStream();
        await using var fileStream = File.OpenRead(file);
        await fileStream.CopyToAsync(memoryStream);
        memoryStream.Position = 0;
        return memoryStream;
    }
}
