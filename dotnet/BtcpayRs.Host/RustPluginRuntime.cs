using System.Reflection;
using Microsoft.Extensions.Logging;
using uniffi.btcpay;
using RustLogLevel = uniffi.btcpay.LogLevel;
using MsLogLevel = Microsoft.Extensions.Logging.LogLevel;

namespace BtcpayRs.Host;

/// <summary>
/// Owns one Rust plugin: loads it, starts and stops it, feeds it events, and carries out the
/// actions it asks for.
/// </summary>
/// <remarks>
/// Deliberately not an <c>IHostedService</c> itself, so it can be driven directly in tests.
/// <c>BtcpayRs.Host.BTCPay</c> wraps it in one.
/// </remarks>
public sealed class RustPluginRuntime : IDisposable
{
    private readonly IPluginBackend _backend;
    private readonly ILogger _logger;
    private readonly string _pluginId;

    private PluginHandle? _handle;
    private HostServicesImpl? _services;
    private bool _started;

    /// <summary>Creates a runtime for the plugin identified by <paramref name="pluginId"/>.</summary>
    public RustPluginRuntime(string pluginId, IPluginBackend backend, ILogger logger)
    {
        _pluginId = pluginId;
        _backend = backend;
        _logger = logger;
    }

    /// <summary>Metadata reported by the Rust library. Available only after <see cref="Start"/>.</summary>
    public PluginMetadata? Metadata { get; private set; }

    /// <summary>
    /// Loads the native library, verifies the ABI, and starts the plugin.
    /// </summary>
    /// <param name="pluginAssembly">
    /// Assembly whose directory holds the native library. Pass the plugin's own assembly.
    /// </param>
    /// <exception cref="PluginLoadException">
    /// The library is missing or incompatible, or the plugin refused to start.
    /// </exception>
    public void Start(Assembly pluginAssembly)
    {
        if (_started) return;

        NativeLoader.Initialize(pluginAssembly);

        _handle = new PluginHandle();
        Metadata = _handle.Metadata();

        // The C# assembly declares identity independently, because BTCPay's packer reads it
        // at packaging time when no native library is loadable. If the two ever disagree,
        // the package is internally inconsistent and would install under the wrong identity.
        VerifyIdentity(Metadata);

        _services = new HostServicesImpl(_pluginId, _backend, _logger);

        try
        {
            _handle.Start(_services);
        }
        catch (PluginException ex)
        {
            throw new PluginLoadException(
                $"plugin '{_pluginId}' failed to start: {ex.Message}", ex);
        }

        _started = true;
        _logger.LogInformation(
            "[{Plugin}] started {Name} v{Version} (ABI {Abi})",
            _pluginId, Metadata.Name, Metadata.Version, _handle.AbiVersion());
    }

    /// <summary>
    /// Stops the plugin. Safe to call more than once, and never throws: shutdown must not be
    /// blockable by plugin code.
    /// </summary>
    public void Stop()
    {
        if (!_started || _handle is null) return;
        _started = false;

        try
        {
            // Rust joins its own threads here, so no callback can arrive after this returns.
            _handle.Stop();
            _logger.LogInformation("[{Plugin}] stopped", _pluginId);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "[{Plugin}] error while stopping; continuing shutdown", _pluginId);
        }
    }

    /// <summary>
    /// Delivers an event to the plugin and performs whatever it asks for in response.
    /// </summary>
    /// <returns>The actions the plugin requested, after they have been carried out.</returns>
    public IReadOnlyList<PluginAction> Dispatch(HostEvent hostEvent)
    {
        if (!_started || _handle is null)
        {
            _logger.LogDebug("[{Plugin}] ignoring event: plugin is not running", _pluginId);
            return Array.Empty<PluginAction>();
        }

        PluginAction[] actions;
        try
        {
            actions = _handle.Handle(hostEvent);
        }
        catch (PluginException ex)
        {
            // A failing handler must not take down the host's event loop.
            _logger.LogError("[{Plugin}] failed to handle {Event}: {Message}",
                _pluginId, hostEvent.GetType().Name, ex.Message);
            return Array.Empty<PluginAction>();
        }

        foreach (var action in actions) Perform(action);
        return actions;
    }

    /// <summary>Asks the plugin to describe its settings form.</summary>
    public UiDocument? SettingsSchema() => Page("settings");

    /// <summary>The pages the plugin offers.</summary>
    public IReadOnlyList<PageInfo> Pages()
    {
        if (_handle is null) return [];
        try
        {
            return _handle.Pages();
        }
        catch (PluginException ex)
        {
            _logger.LogError("[{Plugin}] could not list its pages: {Message}", _pluginId, ex.Message);
            return [];
        }
    }

    /// <summary>Asks the plugin to build one of its pages.</summary>
    /// <returns>The page, or null when the plugin failed or has no such page.</returns>
    public UiDocument? Page(string pageId)
    {
        if (_handle is null) return null;
        try
        {
            return _handle.Page(pageId);
        }
        catch (PluginException ex)
        {
            _logger.LogError("[{Plugin}] could not build page '{Page}': {Message}",
                _pluginId, pageId, ex.Message);
            return null;
        }
    }

    private void Perform(PluginAction action)
    {
        try
        {
            switch (action)
            {
                case PluginAction.SaveSettings save:
                    foreach (var (key, value) in save.Values) _backend.SetSetting(key, value);
                    break;
                case PluginAction.Notify notify:
                    _backend.Notify(notify.Notification);
                    break;
                case PluginAction.SendWebhook webhook:
                    _backend.SendWebhook(webhook.Webhook);
                    break;
                case PluginAction.Log log:
                    _logger.Log(MapLevel(log.Level), "[{Plugin}] {Message}", _pluginId, log.Message);
                    break;
                case PluginAction.Refresh:
                    // Nothing to do here: the UI re-reads the schema on next render.
                    break;
            }
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "[{Plugin}] could not perform {Action}",
                _pluginId, action.GetType().Name);
        }
    }

    private void VerifyIdentity(PluginMetadata metadata)
    {
        if (!string.Equals(metadata.Identifier, _pluginId, StringComparison.Ordinal))
        {
            throw new PluginLoadException(
                $"plugin identity mismatch: the C# assembly declares '{_pluginId}' but the " +
                $"native library reports '{metadata.Identifier}'. The package was assembled " +
                "from mismatched parts; rebuild it.");
        }
    }

    private static MsLogLevel MapLevel(RustLogLevel level) => level switch
    {
        RustLogLevel.Trace => MsLogLevel.Trace,
        RustLogLevel.Debug => MsLogLevel.Debug,
        RustLogLevel.Info => MsLogLevel.Information,
        RustLogLevel.Warn => MsLogLevel.Warning,
        RustLogLevel.Error => MsLogLevel.Error,
        _ => MsLogLevel.Information,
    };

    /// <summary>Stops the plugin and releases the native handle.</summary>
    public void Dispose()
    {
        Stop();
        _handle?.Dispose();
        _handle = null;
    }
}
