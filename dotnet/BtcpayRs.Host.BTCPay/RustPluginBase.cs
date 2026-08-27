using System.Reflection;
using BTCPayServer;
using BTCPayServer.Abstractions.Contracts;
using BTCPayServer.Abstractions.Models;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;

namespace BtcpayRs.Host.BTCPay;

/// <summary>
/// Base class for the small C# assembly that fronts a Rust plugin. A generated subclass
/// supplies the identity constants; everything else is handled here.
/// </summary>
/// <remarks>
/// <para>
/// Identity is declared in C#, not read from Rust, and that is deliberate.
/// <c>BTCPayServer.PluginPacker</c> instantiates this type at packaging time to write
/// <c>&lt;name&gt;.btcpay.json</c>, at which point no native library is loadable. A metadata
/// property that called into Rust would break packaging entirely.
/// </para>
/// <para>
/// <see cref="RustPluginRuntime"/> checks the two agree at startup, so the duplication cannot
/// drift silently.
/// </para>
/// </remarks>
public abstract class RustPluginBase : BaseBTCPayServerPlugin
{
    /// <summary>
    /// How often the plugin receives <c>HostEvent.Tick</c>. Override to change or, with
    /// <see cref="TimeSpan.Zero"/>, disable it.
    /// </summary>
    protected virtual TimeSpan TickInterval => TimeSpan.FromMinutes(1);

    /// <summary>
    /// Registers the plugin's runtime. Override to add your own services, calling
    /// <c>base.Execute(services)</c> first.
    /// </summary>
    /// <summary>
    /// Where this plugin's settings page lives, or null when it has none.
    /// </summary>
    /// <remarks>
    /// Overridden by the generated plugin class when the plugin describes a settings page.
    /// </remarks>
    public virtual string? SettingsPath => null;

    /// <summary>
    /// How long the host waits for this plugin to stop before abandoning it.
    /// </summary>
    /// <remarks>
    /// Override when a plugin needs longer to finish safely. "Close a socket" and "finish an
    /// in-flight swap" are not the same wait.
    /// </remarks>
    public virtual TimeSpan ShutdownTimeout => TimeSpan.FromSeconds(30);

    /// <summary>The partial that adds btcpay-rs plugins to the server menu.</summary>
    /// <remarks>
    /// A full path, not a name. Resolving by name searches the view location formats and
    /// does not find views compiled into this assembly, whereas an explicit path does.
    /// </remarks>
    private const string NavPartial = "~/Views/Shared/_BtcpayRsServerNav.cshtml";

    public override void Execute(IServiceCollection services)
    {
        var pluginAssembly = GetType().Assembly;
        var identifier = Identifier;
        var tickInterval = TickInterval;
        var shutdownTimeout = ShutdownTimeout;
        var settingsPath = SettingsPath;

        // Per plugin: BTCPay's handler lookup is a ToDictionary on NotificationType, so a
        // duplicate throws and breaks notifications server-wide.
        var displayName = Name;
        services.AddSingleton<BTCPayServer.Abstractions.Contracts.INotificationHandler>(
            _ => new RustPluginNotification.Handler(identifier, displayName));

        services.AddSingleton(provider => new RustPluginHostedService(
            identifier,
            pluginAssembly,
            provider.GetRequiredService<ISettingsRepository>(),
            provider.GetRequiredService<EventAggregator>(),
            provider.GetRequiredService<Microsoft.Extensions.Logging.ILogger<RustPluginHostedService>>(),
            // DataDirectories is not a registered service: BTCPay builds it from
            // configuration where it needs one, and so do we. Asking DI for it throws, which
            // crashes the plugin at startup and gets it disabled.
            new BTCPayServer.Configuration.DataDirectories().Configure(
                provider.GetRequiredService<Microsoft.Extensions.Configuration.IConfiguration>()),
            tickInterval,
            shutdownTimeout,
            provider.GetService<BTCPayServer.Services.Notifications.NotificationSender>())
        {
            SettingsPath = settingsPath,
        });

        // Registered once however many btcpay-rs plugins are installed: the partial lists
        // them all, so registering per plugin would render it once per plugin.
        var alreadyRegistered = services.Any(descriptor =>
            descriptor.ServiceType == typeof(IUIExtension)
            && descriptor.ImplementationInstance is IUIExtension extension
            && extension.Partial == NavPartial);

        if (!alreadyRegistered)
        {
            services.AddUIExtension("server-nav", NavPartial);
        }

        services.AddSingleton<IHostedService>(provider =>
            provider.GetRequiredService<RustPluginHostedService>());

        base.Execute(services);
    }
}
