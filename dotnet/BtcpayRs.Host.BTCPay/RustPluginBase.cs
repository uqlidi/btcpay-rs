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
    public override void Execute(IServiceCollection services)
    {
        var pluginAssembly = GetType().Assembly;
        var identifier = Identifier;
        var tickInterval = TickInterval;

        services.AddSingleton(provider => new RustPluginHostedService(
            identifier,
            pluginAssembly,
            provider.GetRequiredService<ISettingsRepository>(),
            provider.GetRequiredService<EventAggregator>(),
            provider.GetRequiredService<Microsoft.Extensions.Logging.ILogger<RustPluginHostedService>>(),
            tickInterval));

        services.AddSingleton<IHostedService>(provider =>
            provider.GetRequiredService<RustPluginHostedService>());

        base.Execute(services);
    }
}
