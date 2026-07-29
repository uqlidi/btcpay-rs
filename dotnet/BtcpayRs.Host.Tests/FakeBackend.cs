using uniffi.btcpay;

namespace BtcpayRs.Host.Tests;

/// <summary>In-memory <see cref="IPluginBackend"/> that records what the plugin asked for.</summary>
internal sealed class FakeBackend : IPluginBackend
{
    public readonly Dictionary<string, string> Settings = new();
    public readonly Dictionary<string, byte[]> Store = new();
    public readonly List<Notification> Notifications = new();
    public readonly List<WebhookRequest> Webhooks = new();

    /// <summary>When set, every operation throws this, simulating a broken host.</summary>
    public Func<Exception>? Fault;

    public string? GetSetting(string key)
    {
        Throw();
        return Settings.TryGetValue(key, out var v) ? v : null;
    }

    public void SetSetting(string key, string value)
    {
        Throw();
        Settings[key] = value;
    }

    public byte[]? StoreGet(string key)
    {
        Throw();
        return Store.TryGetValue(key, out var v) ? v : null;
    }

    public void StorePut(string key, byte[] value)
    {
        Throw();
        Store[key] = value;
    }

    public void StoreDelete(string key)
    {
        Throw();
        Store.Remove(key);
    }

    public void Notify(Notification notification)
    {
        Throw();
        Notifications.Add(notification);
    }

    public void SendWebhook(WebhookRequest webhook)
    {
        Throw();
        Webhooks.Add(webhook);
    }

    private void Throw()
    {
        if (Fault is not null) throw Fault();
    }
}
