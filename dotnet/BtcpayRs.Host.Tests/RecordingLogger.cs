using Microsoft.Extensions.Logging;

namespace BtcpayRs.Host.Tests;

/// <summary>Captures log entries so tests can assert that failures were reported, not lost.</summary>
internal sealed class RecordingLogger : ILogger
{
    public readonly List<(LogLevel Level, string Message, Exception? Exception)> Entries = new();

    public IDisposable? BeginScope<TState>(TState state) where TState : notnull => null;

    public bool IsEnabled(LogLevel logLevel) => true;

    public void Log<TState>(LogLevel logLevel, EventId eventId, TState state, Exception? exception,
        Func<TState, Exception?, string> formatter)
    {
        lock (Entries)
        {
            Entries.Add((logLevel, formatter(state, exception), exception));
        }
    }

    public bool HasError(string substring)
    {
        lock (Entries)
        {
            return Entries.Any(e => e.Level == LogLevel.Error && e.Message.Contains(substring));
        }
    }
}
