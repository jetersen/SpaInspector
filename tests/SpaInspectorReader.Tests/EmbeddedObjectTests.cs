using System.Buffers.Binary;
using System.Text;
using System.Text.Json;
using SpaTestUtils;

namespace SpaInspectorReader.Tests;

public class EmbeddedObjectTests
{
    private static byte[] ObjectPayload(ushort classId, byte[] body, params byte[] trailing)
    {
        var payload = new byte[6 + body.Length + trailing.Length];
        BinaryPrimitives.WriteUInt16LittleEndian(payload, classId);
        BinaryPrimitives.WriteUInt32LittleEndian(payload.AsSpan(2, 4), (uint)body.Length);
        body.CopyTo(payload, 6);
        trailing.CopyTo(payload, 6 + body.Length);
        return payload;
    }

    private static Spa Document(params SpaFixture.Section[] objects) => SpaParser.Parse(SpaFixture.Build(
        new[] { new SpaFixture.Section(2, SpaFixture.Header()), new SpaFixture.Section(3, SpaFixture.Floats(1, 2, 3, 4)) }.Concat(objects).ToArray()));

    [Fact]
    public void CollectionDurationUsesUnsignedCentisecondsWithoutConflatingSignalStrength()
    {
        var header = SpaFixture.Header();
        BinaryPrimitives.WriteUInt32LittleEndian(header.AsSpan(68, 4), 12345);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(72, 4), 42);
        var document = SpaParser.Parse(SpaFixture.Build(new(2, header), new(3, SpaFixture.Floats(1, 2, 3, 4))));
        Assert.Equal(12345u, document.Series[0].Metadata.CollectionDurationCentiseconds);
        Assert.Equal(42f, document.Series[0].Metadata.SignalStrength);
    }

    [Fact]
    public void ExperimentObjectDecodesOnlyKnownDescriptorsAndKeepsBodyAndPadding()
    {
        var body = new byte[925];
        Encoding.UTF8.GetBytes("short.exp").CopyTo(body, 10 - 6);
        Encoding.UTF8.GetBytes("Experiment é").CopyTo(body, 90 - 6);
        Encoding.UTF8.GetBytes("Description\r\n  ").CopyTo(body, 154 - 6);
        Encoding.UTF8.GetBytes("Unassigned text").CopyTo(body, 413 - 6);
        Encoding.UTF8.GetBytes("long/experiment.exp").CopyTo(body, 671 - 6);
        var payload = ObjectPayload(121, body, 0xAB);
        var spa = Document(new SpaFixture.Section(130, payload, 9));
        var instance = Assert.Single(spa.EmbeddedObjects);
        Assert.Equal(2, instance.SectionIndex);
        Assert.Equal("experiment", instance.Kind);
        Assert.Equal(121, instance.ClassId);
        Assert.Equal(9, instance.SecondarySelector);
        Assert.Equal(body, instance.Body);
        Assert.Equal(new byte[] { 0xAB }, instance.TrailingBytes);
        Assert.Equal(925u, instance.DeclaredBodyLength);
        Assert.Equal(5, instance.Attributes.Count);
        Assert.Equal("short.exp", instance.Attributes.Single(attribute => attribute.Id == 2).Value);
        Assert.Equal("Experiment é", instance.Attributes.Single(attribute => attribute.Id == 3).Value);
        Assert.Equal("Description\r\n  ", instance.Attributes.Single(attribute => attribute.Id == 4).Value);
        Assert.Null(instance.Attributes.Single(attribute => attribute.Id == 7).Name);
        Assert.Equal("long/experiment.exp", instance.Attributes.Single(attribute => attribute.Id == 49).Value);
        Assert.Equal(671u, instance.Attributes.Single(attribute => attribute.Id == 49).Offset);
        Assert.Equal(payload, spa.Sections[2].Payload);
    }

    [Fact]
    public void AtrObjectLabelsVerifiedControlsAndPreservesReservedAndNonFiniteValues()
    {
        var body = SpaFixture.Floats(1, -2, 3, float.NaN, float.PositiveInfinity, float.NegativeInfinity, 7);
        var spa = Document(new SpaFixture.Section(130, ObjectPayload(159, body, 0, 0), 2));
        var instance = Assert.Single(spa.EmbeddedObjects);
        Assert.Equal("advanced_atr", instance.Kind);
        Assert.Equal(7, instance.Attributes.Count);
        Assert.Equal(new string[] { "applied", "sampleRefractiveIndex", "crystalRefractiveIndex", "numberOfBounces", "angleOfIncidenceDegrees", null, null }, instance.Attributes.Select(a => a.Name));
        Assert.All(instance.Attributes, attribute => Assert.Equal("float32", attribute.Type));
        Assert.Equal(6u, instance.Attributes[0].Offset);
        Assert.Equal(1f, Assert.IsType<float>(instance.Attributes[0].Value));
        using var json = JsonDocument.Parse(spa.ToJson());
        var attributes = json.RootElement.GetProperty("embeddedObjects")[0].GetProperty("attributes");
        Assert.Equal("NaN", attributes[3].GetProperty("value").GetString());
        Assert.Equal("Infinity", attributes[4].GetProperty("value").GetString());
        Assert.Equal("-Infinity", attributes[5].GetProperty("value").GetString());
    }

    [Fact]
    public void InstrumentSpecificationUsesFullWidthUnsignedFieldAndUnlabelledFloat()
    {
        var body = new byte[6];
        BinaryPrimitives.WriteUInt16LittleEndian(body, ushort.MaxValue);
        BinaryPrimitives.WriteSingleLittleEndian(body.AsSpan(2, 4), 12.5f);
        var instance = Assert.Single(Document(new SpaFixture.Section(130, ObjectPayload(166, body))).EmbeddedObjects);
        Assert.Equal("instrument_specification", instance.Kind);
        Assert.Equal(ushort.MaxValue, Assert.IsType<ushort>(instance.Attributes[0].Value));
        Assert.Equal(12.5f, Assert.IsType<float>(instance.Attributes[1].Value));
        Assert.All(instance.Attributes, attribute => Assert.Null(attribute.Name));
    }

    [Fact]
    public void CachedClassIsEffectiveButDisagreementSuppressesAttributeInterpretation()
    {
        var body = new byte[925];
        var cached = (158u << 16) | 513u;
        var spa = Document(new SpaFixture.Section(130, ObjectPayload(121, body), 4, cached));
        var instance = Assert.Single(spa.EmbeddedObjects);
        Assert.Equal(158, instance.ClassId);
        Assert.Equal(121, instance.PayloadClassId);
        Assert.Equal((ushort)158, instance.CachedClassId);
        Assert.Equal((ushort)513, instance.CachedBodyWord);
        Assert.Equal("instrument_status", instance.Kind);
        Assert.Empty(instance.Attributes);
        Assert.Contains(spa.Warnings, warning => warning.Contains("disagrees with payload class"));
        Assert.Equal(body, instance.Body);
        var matching = Document(new SpaFixture.Section(130, ObjectPayload(121, body), 4, (121u << 16) | 513u));
        Assert.Equal(5, matching.EmbeddedObjects[0].Attributes.Count);
    }

    [Fact]
    public void AttributeBoundsUseDeclaredBodyRatherThanTrailingAttachmentBytes()
    {
        var body = SpaFixture.Floats(7);
        var trailing = SpaFixture.Floats(8, 9, 10, 11, 12, 13);
        var instance = Assert.Single(Document(new SpaFixture.Section(130, ObjectPayload(159, body, trailing))).EmbeddedObjects);
        Assert.Single(instance.Attributes);
        Assert.Equal(7f, Assert.IsType<float>(instance.Attributes[0].Value));
        Assert.Equal(trailing, instance.TrailingBytes);
        var shortExperiment = Document(new SpaFixture.Section(130, ObjectPayload(121, new byte[80], new byte[1000])));
        Assert.Empty(shortExperiment.EmbeddedObjects[0].Attributes);
    }

    [Fact]
    public void InvalidAndUnknownObjectsRetainRawBytesWithoutRejectingValidSpectra()
    {
        var invalid = ObjectPayload(121, []);
        BinaryPrimitives.WriteUInt32LittleEndian(invalid.AsSpan(2, 4), uint.MaxValue);
        var spa = Document(new SpaFixture.Section(130, [1, 2]), new SpaFixture.Section(130, invalid, 1), new SpaFixture.Section(130, ObjectPayload(501, [0x80, 0xFF]), 2));
        var instance = Assert.Single(spa.EmbeddedObjects);
        Assert.Equal("class_501", instance.Kind);
        Assert.Equal(new byte[] { 0x80, 0xFF }, instance.Body);
        Assert.Empty(instance.Attributes);
        Assert.Equal(2, spa.Warnings.Count(warning => warning.Contains("raw section is preserved")));
        Assert.Equal(new byte[] { 1, 2 }, spa.Sections[2].Payload);
        Assert.Equal(invalid, spa.Sections[3].Payload);
        using var json = JsonDocument.Parse(spa.ToJson());
        Assert.Equal(spa.SourceBytes, json.RootElement.GetProperty("sourceBytes").GetBytesFromBase64());
    }

    [Fact]
    public void ObjectStringsFollowTheSelectedLegacyEncodingPolicy()
    {
        var body = new byte[925]; body[4] = 0x80; body[5] = 0xE9;
        var bytes = SpaFixture.Build(new(2, SpaFixture.Header()), new(3, SpaFixture.Floats(1, 2, 3, 4)), new SpaFixture.Section(130, ObjectPayload(121, body)));
        var automatic = SpaParser.Parse(bytes);
        Assert.Equal("€é", automatic.EmbeddedObjects[0].Attributes.Single(attribute => attribute.Id == 2).Value);
        Assert.Contains(automatic.Warnings, warning => warning.Contains("Object 2 attribute 2"));
        var latin = SpaParser.Parse(bytes, new() { TextEncoding = SpaTextEncoding.Latin1 });
        Assert.Equal("\u0080é", latin.EmbeddedObjects[0].Attributes.Single(attribute => attribute.Id == 2).Value);
        Assert.Throws<InvalidDataException>(() => SpaParser.Parse(bytes, new() { TextEncoding = SpaTextEncoding.Utf8 }));
    }

    [Fact]
    public void VerifiedScalarMetadataUsesDistinctOffsetsAndExactProcessingSelectors()
    {
        var header = SpaFixture.Header();
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(56, 4), 3.5f);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(80, 4), 15800.25f);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(84, 4), 1.25f);
        BinaryPrimitives.WriteSingleLittleEndian(header.AsSpan(96, 4), 15700.5f);
        var code = new byte[4]; BinaryPrimitives.WriteUInt32LittleEndian(code, 1234);
        var wrongCode = new byte[4]; BinaryPrimitives.WriteUInt32LittleEndian(wrongCode, 5678);
        var spa = SpaParser.Parse(SpaFixture.Build(new(2, header), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(128, wrongCode), new(384, wrongCode, 1), new(384, code)));
        var metadata = spa.Spectrum!.Metadata;
        Assert.Equal(3.5f, metadata.BackgroundGain);
        Assert.Equal(15800.25f, metadata.LaserFrequencyCmInverse);
        Assert.Equal(1.25f, metadata.SampleSpacing);
        Assert.Equal(15700.5f, metadata.RamanLaserFrequencyCmInverse);
        Assert.Equal(1234u, metadata.ProcessingAlgorithmCode);
        var shortSpa = SpaParser.Parse(SpaFixture.Build(new(2, header[..24]), new(3, SpaFixture.Floats(1, 2, 3, 4)), new(384, [1, 2, 3])));
        Assert.Null(shortSpa.Spectrum!.Metadata.BackgroundGain);
        Assert.Null(shortSpa.Spectrum.Metadata.LaserFrequencyCmInverse);
        Assert.Null(shortSpa.Spectrum.Metadata.SampleSpacing);
        Assert.Null(shortSpa.Spectrum.Metadata.RamanLaserFrequencyCmInverse);
        Assert.Null(shortSpa.Spectrum.Metadata.ProcessingAlgorithmCode);
    }
}
