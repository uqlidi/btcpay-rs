using BTCPayServer.Abstractions.Contracts;
using BtcpayRs.Host.BTCPay;

namespace BTCPayServer.Plugins.Hello;

/// <summary>
/// The whole C# side of this plugin: identity, and nothing else. All behaviour lives in
/// <c>examples/hello-plugin/src/lib.rs</c>.
/// </summary>
/// <remarks>
/// These constants are checked against the Rust library's own metadata at startup, so the two
/// cannot drift apart unnoticed. They are declared here rather than read from Rust because
/// BTCPay's packer instantiates this type when no native library is loadable.
/// </remarks>
public class HelloPlugin : RustPluginBase
{
    public override string Identifier => "BTCPayServer.Plugins.Hello";

    public override string Name => "Hello";

    public override string Description => "Example plugin demonstrating btcpay-rs.";

    public override IBTCPayServerPlugin.PluginDependency[] Dependencies { get; } =
    [
        new() { Identifier = nameof(BTCPayServer), Condition = ">=2.4.0" }
    ];
}
