namespace SpaInspectorReader;

/// <summary>Stream adapter. Caller-owned streams remain open by default.</summary>
public record Read : IDisposable
{
    private readonly Stream _stream;
    private readonly bool _leaveOpen;
    private readonly SpaReadOptions? _options;

    public Read(Stream stream, bool leaveOpen = true, SpaReadOptions? options = null)
    {
        ArgumentNullException.ThrowIfNull(stream);
        _stream = stream;
        _leaveOpen = leaveOpen;
        _options = options;
    }

    public Spa ReadBinaryToSpa()
    {
        if (_stream.CanSeek) _stream.Position = 0;
        using var buffer = new MemoryStream();
        _stream.CopyTo(buffer);
        return SpaParser.Parse(buffer.GetBuffer().AsSpan(0, checked((int)buffer.Length)), _options);
    }

    public void Dispose()
    {
        if (!_leaveOpen) _stream.Dispose();
        GC.SuppressFinalize(this);
    }
}
