using System.Reflection;
using BTCPayServer;
using BTCPayServer.Abstractions.Contracts;
using BTCPayServer.Events;
using BTCPayServer.Services.Invoices;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using uniffi.btcpay;

namespace BtcpayRs.Host.BTCPay;

/// <summary>
/// Runs a Rust plugin for as long as BTCPay is running, and forwards BTCPay's events to it.
/// </summary>
public sealed class RustPluginHostedService : IHostedService, IDisposable
{
    private readonly EventAggregator _events;
    private readonly ILogger<RustPluginHostedService> _logger;
    private readonly RustPluginRuntime _runtime;
    private readonly Assembly _pluginAssembly;
    private readonly TimeSpan _tickInterval;

    private readonly List<IEventAggregatorSubscription> _subscriptions = new();
    private Timer? _tick;

    /// <summary>Creates the service for one plugin.</summary>
    /// <param name="pluginAssembly">
    /// The plugin's own assembly; its directory is where the native library is found.
    /// </param>
    /// <param name="tickInterval">
    /// How often to deliver <c>HostEvent.Tick</c>. <see cref="TimeSpan.Zero"/> disables it.
    /// </param>
    public RustPluginHostedService(
        string pluginId,
        Assembly pluginAssembly,
        ISettingsRepository settings,
        EventAggregator events,
        ILogger<RustPluginHostedService> logger,
        TimeSpan? tickInterval = null)
    {
        _events = events;
        _logger = logger;
        _pluginAssembly = pluginAssembly;
        _tickInterval = tickInterval ?? TimeSpan.FromMinutes(1);
        _runtime = new RustPluginRuntime(pluginId, new SettingsRepositoryBackend(pluginId, settings, logger), logger);
        PluginIdentifier = pluginId;
    }

    /// <inheritdoc />
    /// <summary>The plugin this service runs.</summary>
    public string PluginIdentifier { get; }

    /// <summary>
    /// Where this plugin's settings page lives, or null when it has none.
    /// </summary>
    /// <remarks>
    /// Set from the generated plugin class rather than derived here, so it cannot disagree
    /// with the route on the generated controller.
    /// </remarks>
    public string? SettingsPath { get; init; }

    /// <summary>The plugin's display name, once it has started.</summary>
    public string PluginName => _runtime.Metadata?.Name ?? PluginIdentifier;

    /// <summary>
    /// The settings page the plugin describes, rebuilt on every request so it reflects what
    /// is currently stored.
    /// </summary>
    public UiPage SettingsPage()
    {
        var document = _runtime.SettingsSchema();
        return document is null ? UiPage.Empty : UiPage.Parse(document.DocumentJson);
    }

    /// <summary>
    /// Hands a settings submission to the plugin and carries out whatever it asks for.
    /// </summary>
    /// <returns>
    /// The actions the plugin requested. Empty means it refused the submission, which it
    /// reports by logging.
    /// </returns>
    public IReadOnlyList<uniffi.btcpay.PluginAction> SubmitSettings(
        IReadOnlyDictionary<string, string> values)
    {
        return _runtime.Dispatch(new uniffi.btcpay.HostEvent.SettingsUpdated(
            new Dictionary<string, string>(values)));
    }

    public Task StartAsync(CancellationToken cancellationToken)
    {
        // A plugin that cannot start must not take BTCPay down with it. BTCPay catches this
        // per-plugin and disables the offending one, which is the behaviour we want.
        _runtime.Start(_pluginAssembly);

        _subscriptions.Add(_events.Subscribe<InvoiceEvent>((_, e) => OnInvoiceEvent(e)));

        if (_tickInterval > TimeSpan.Zero)
        {
            _tick = new Timer(_ => SafeDispatch(new HostEvent.Tick()), null, _tickInterval, _tickInterval);
        }

        return Task.CompletedTask;
    }

    /// <inheritdoc />
    public Task StopAsync(CancellationToken cancellationToken)
    {
        // Order matters: stop feeding events first, then stop the plugin, so nothing is
        // delivered to a plugin that is shutting down.
        foreach (var subscription in _subscriptions) subscription.Dispose();
        _subscriptions.Clear();

        _tick?.Dispose();
        _tick = null;

        _runtime.Stop();
        return Task.CompletedTask;
    }

    private void OnInvoiceEvent(InvoiceEvent e)
    {
        var invoice = ToSummary(e.Invoice);

        // Every invoice event is forwarded. Deciding which ones matter belongs to the plugin,
        // and InvoiceTrigger.Other carries anything not modelled yet, so a BTCPay upgrade
        // adding events cannot silently drop them here.
        HostEvent mapped = e.Name == InvoiceEvent.Created
            ? new HostEvent.InvoiceCreated(invoice)
            : new HostEvent.InvoiceStatusChanged(invoice, ToTrigger(e.Name));

        SafeDispatch(mapped);
    }

    /// <summary>
    /// Maps a BTCPay invoice event name onto the contract's trigger.
    /// </summary>
    /// <remarks>
    /// BTCPay reports what happened, never the status the invoice came from, so the contract
    /// names the cause instead of describing a transition it cannot know.
    /// </remarks>
    private static InvoiceTrigger ToTrigger(string eventName) => eventName switch
    {
        InvoiceEvent.PaidInFull => new InvoiceTrigger.PaidInFull(),
        InvoiceEvent.Confirmed => new InvoiceTrigger.Confirmed(),
        InvoiceEvent.Completed => new InvoiceTrigger.Completed(),
        InvoiceEvent.Expired => new InvoiceTrigger.Expired(),
        InvoiceEvent.ExpiredPaidPartial => new InvoiceTrigger.ExpiredPaidPartial(),
        InvoiceEvent.MarkedCompleted => new InvoiceTrigger.MarkedCompleted(),
        InvoiceEvent.MarkedInvalid => new InvoiceTrigger.MarkedInvalid(),
        InvoiceEvent.ReceivedPayment => new InvoiceTrigger.ReceivedPayment(),
        InvoiceEvent.PaymentSettled => new InvoiceTrigger.PaymentSettled(),
        _ => new InvoiceTrigger.Other(eventName),
    };

    private static InvoiceSummary ToSummary(InvoiceEntity invoice) => new(
        invoice.Id,
        invoice.StoreId,
        invoice.Status.ToString(),
        // Decimal string, never a float: this is money.
        invoice.Price.ToString(System.Globalization.CultureInfo.InvariantCulture),
        invoice.Currency);

    /// <summary>
    /// Dispatches without letting a failure escape into BTCPay's event loop or timer thread.
    /// </summary>
    private void SafeDispatch(HostEvent hostEvent)
    {
        try
        {
            _runtime.Dispatch(hostEvent);
        }
        catch (Exception ex)
        {
            _logger.LogError(ex, "failed to deliver {Event} to the plugin", hostEvent.GetType().Name);
        }
    }

    /// <inheritdoc />
    public void Dispose()
    {
        _tick?.Dispose();
        _runtime.Dispose();
    }
}
