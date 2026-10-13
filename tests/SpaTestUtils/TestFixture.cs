namespace SpaTestUtils;

public static class TestFixture
{
    public static string TestFile(string file) => Path.Combine(AppContext.BaseDirectory, "TestFiles", file);
    public static string BenchmarkTestFile(string file) => TestFile(file);
}
