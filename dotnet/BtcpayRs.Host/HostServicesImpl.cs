using Microsoft.Extensions.Logging;
using uniffi.btcpay;
using RustLogLevel = uniffi.btcpay.LogLevel;
using MsLogLevel = Microsoft.Extensions.Logging.LogLevel;

namespace BtcpayRs.Host;

/// <summary>
/// Implements the Rust <c>HostServices</c> trait on top of an <see cref="IPluginBackend"/>.
/// </summary>
/// <remarks>
/// <para>
/// <b>No method here may let an exception escape.</b> uniffi turns an unexpected exception
/// from a foreign implementation into a Rust panic. On a call the plugin makes from a
/// background thread, that panic unwinds and kills the thread, so the plugin stops working
/// with no error surfaced anywhere. A plugin that logged one line and then went permanently
/// silent would be the visible symptom.
/// </para>
/// <para>
/// So every method catches everything, logs it, and returns a value or a
/// <see cref="HostException"/>. The fallible operations are declared to return
/// <c>Result</c> in Rust precisely so failure can be reported without a panic.
/// </para>
/// </remarks>
public sealed class HostServicesImpl : HostServices
{
    private readonly IPluginBackend _backend;
    private readonly ILogger _logger;
    private readonly string _pluginId;

    /// <summary>Creates the bridge for one plugin.</summary>
    public HostServicesImpl(string pluginId, IPluginBackend backend, ILogger logger)
    {
        _pluginId = pluginId;
        _backend = backend;
        _logger = logger;
    }

    /// <inheritdoc />
    public string DataDir() =>
        // A fallback path would be worse than an empty string: the plugin would write files
        // somewhere unexpected and an operator would not find them. Empty is checkable.
        Safe(nameof(DataDir), () => _backend.DataDirectory, string.Empty);

    /// <inheritdoc />
    public string? GetSetting(string key) =>
        Safe(nameof(GetSetting), () => _backend.GetSetting(key), null);

    /// <inheritdoc />
    public void SetSetting(string key, string value) =>
        SafeFallible(nameof(SetSetting), () => _backend.SetSetting(key, value));

    /// <inheritdoc />
    public byte[]? StoreGet(string key) =>
        Safe(nameof(StoreGet), () => _backend.StoreGet(key), null);

    /// <inheritdoc />
    public void StorePut(string key, byte[] value) =>
        SafeFallible(nameof(StorePut), () => _backend.StorePut(key, value));

    /// <inheritdoc />
    public void StoreDelete(string key) =>
        SafeFallible(nameof(StoreDelete), () => _backend.StoreDelete(key));

    /// <inheritdoc />
    public void Log(RustLogLevel level, string message) =>
        Safe(nameof(Log), () =>
        {
            _logger.Log(Map(level), "[{Plugin}] {Message}", _pluginId, message);
            return true;
        }, false);

    /// <inheritdoc />
    public void EmitNotification(Notification notification) =>
        SafeFallible(nameof(EmitNotification), () => _backend.Notify(notification));

    /// <inheritdoc />
    public void SendWebhook(WebhookRequest webhook) =>
        SafeFallible(nameof(SendWebhook), () => _backend.SendWebhook(webhook));

    /// <summary>
    /// Runs an infallible host call, substituting <paramref name="fallback"/> on failure.
    /// Never throws.
    /// </summary>
    private T Safe<T>(string operation, Func<T> action, T fallback)
    {
        try
        {
            return action();
        }
        catch (Exception ex)
        {
            LogSwallowed(operation, ex);
            return fallback;
        }
    }

    /// <summary>
    /// Runs a fallible host call, reporting failure to Rust as a <see cref="HostException"/>
    /// rather than as a panic.
    /// </summary>
    private void SafeFallible(string operation, Action action)
    {
        try
        {
            action();
        }
        catch (Exception ex)
        {
            LogSwallowed(operation, ex);
            throw new HostException.Failed($"{operation} failed: {ex.Message}");
        }
    }

    private void LogSwallowed(string operation, Exception ex) =>
        _logger.LogError(ex,
            "[{Plugin}] host service {Operation} threw; contained so the plugin's event loop " +
            "keeps running", _pluginId, operation);

    private static MsLogLevel Map(RustLogLevel level) => level switch
    {
        RustLogLevel.Trace => MsLogLevel.Trace,
        RustLogLevel.Debug => MsLogLevel.Debug,
        RustLogLevel.Info => MsLogLevel.Information,
        RustLogLevel.Warn => MsLogLevel.Warning,
        RustLogLevel.Error => MsLogLevel.Error,
        _ => MsLogLevel.Information,
    };
}
