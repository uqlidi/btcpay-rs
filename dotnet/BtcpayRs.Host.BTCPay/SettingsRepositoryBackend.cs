using BTCPayServer.Abstractions.Contracts;
using Microsoft.Extensions.Logging;
using uniffi.btcpay;

namespace BtcpayRs.Host.BTCPay;

/// <summary>
/// Backs a plugin's settings and key/value store with BTCPay's <see cref="ISettingsRepository"/>.
/// </summary>
/// <remarks>
/// <para>
/// BTCPay stores settings as typed objects keyed by name, so both stores are held in one
/// per-plugin <see cref="PluginState"/> document, namespaced by the plugin identifier. This
/// avoids giving every plugin an EF <c>DbContext</c> and migration purely to hold a handful of
/// key/value pairs.
/// </para>
/// <para>
/// The trade-off is that the store is loaded and saved whole, so it suits configuration and
/// modest state, not high-volume or high-frequency data. A plugin needing that should own its
/// own storage; revisit if one does.
/// </para>
/// </remarks>
public sealed class SettingsRepositoryBackend : IPluginBackend
{
    private readonly ISettingsRepository _settings;
    private readonly ILogger _logger;
    private readonly string _pluginId;
    private readonly SemaphoreSlim _gate = new(1, 1);

    private PluginState? _cache;

    /// <summary>Creates the backend for one plugin.</summary>
    public SettingsRepositoryBackend(string pluginId, ISettingsRepository settings, ILogger logger)
    {
        _pluginId = pluginId;
        _settings = settings;
        _logger = logger;
    }

    /// <summary>Persisted shape of a plugin's settings and key/value store.</summary>
    public sealed class PluginState
    {
        /// <summary>Operator-editable settings.</summary>
        public Dictionary<string, string> Settings { get; set; } = new();

        /// <summary>The plugin's private key/value store, base64 encoded.</summary>
        public Dictionary<string, string> Store { get; set; } = new();
    }

    /// <inheritdoc />
    public string? GetSetting(string key) =>
        Load().Settings.TryGetValue(key, out var value) ? value : null;

    /// <inheritdoc />
    public void SetSetting(string key, string value) =>
        Mutate(state => state.Settings[key] = value);

    /// <inheritdoc />
    public byte[]? StoreGet(string key) =>
        Load().Store.TryGetValue(key, out var value) ? Convert.FromBase64String(value) : null;

    /// <inheritdoc />
    public void StorePut(string key, byte[] value) =>
        Mutate(state => state.Store[key] = Convert.ToBase64String(value));

    /// <inheritdoc />
    public void StoreDelete(string key) => Mutate(state => state.Store.Remove(key));

    /// <inheritdoc />
    public void Notify(Notification notification)
    {
        // Raising a BTCPay notification requires wiring its notification pipeline, which is
        // deliberately left for the UI milestone rather than guessed at here. Logging keeps
        // the information reachable meanwhile.
        _logger.LogInformation("[{Plugin}] notification: {Title} - {Body}",
            _pluginId, notification.Title, notification.Body);
    }

    /// <inheritdoc />
    public void SendWebhook(WebhookRequest webhook)
    {
        _logger.LogInformation("[{Plugin}] webhook {EventType}: {Payload}",
            _pluginId, webhook.EventType, webhook.PayloadJson);
    }

    /// <summary>
    /// Reads the plugin's state, caching it. The Rust side calls synchronously, so the async
    /// repository is bridged here rather than in every caller.
    /// </summary>
    private PluginState Load()
    {
        if (_cache is not null) return _cache;

        _gate.Wait();
        try
        {
            _cache ??= _settings.GetSettingAsync<PluginState>(SettingsName).GetAwaiter().GetResult()
                       ?? new PluginState();
            return _cache;
        }
        finally
        {
            _gate.Release();
        }
    }

    private void Mutate(Action<PluginState> change)
    {
        _gate.Wait();
        try
        {
            var state = _cache ??=
                _settings.GetSettingAsync<PluginState>(SettingsName).GetAwaiter().GetResult()
                ?? new PluginState();
            change(state);
            _settings.UpdateSetting(state, SettingsName).GetAwaiter().GetResult();
        }
        finally
        {
            _gate.Release();
        }
    }

    private string SettingsName => $"BtcpayRs.{_pluginId}";
}
