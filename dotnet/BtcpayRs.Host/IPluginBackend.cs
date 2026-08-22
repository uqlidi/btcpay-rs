using uniffi.btcpay;

namespace BtcpayRs.Host;

/// <summary>
/// Everything the Rust side can ask of the host, expressed without reference to BTCPay.
/// </summary>
/// <remarks>
/// This exists so the whole runtime can be exercised against an in-memory fake. The BTCPay
/// implementation lives in <c>BtcpayRs.Host.BTCPay</c> and maps these onto
/// <c>ISettingsRepository</c>, the plugin's <c>DbContext</c>, notifications and webhooks.
/// </remarks>
public interface IPluginBackend
{
    /// <summary>
    /// A directory the plugin may write files in, created before it starts.
    /// </summary>
    /// <remarks>
    /// Inside BTCPay's own data directory, so an operator's backups and volume mounts already
    /// cover it. Not transactional and not migrated: a plugin owns whatever it writes here.
    /// </remarks>
    string DataDirectory { get; }

    /// <summary>Reads an operator-editable setting.</summary>
    string? GetSetting(string key);

    /// <summary>Writes an operator-editable setting.</summary>
    void SetSetting(string key, string value);

    /// <summary>Reads from the plugin's private key/value store.</summary>
    byte[]? StoreGet(string key);

    /// <summary>Writes to the plugin's private key/value store.</summary>
    void StorePut(string key, byte[] value);

    /// <summary>Removes a key from the plugin's private store.</summary>
    void StoreDelete(string key);

    /// <summary>Raises a notification for the operator.</summary>
    void Notify(Notification notification);

    /// <summary>Delivers a webhook through the host's webhook machinery.</summary>
    void SendWebhook(WebhookRequest webhook);
}
