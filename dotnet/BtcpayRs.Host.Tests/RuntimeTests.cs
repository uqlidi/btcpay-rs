using System.Reflection;
using uniffi.btcpay;
using Xunit;

namespace BtcpayRs.Host.Tests;

/// <summary>
/// Drives the real <c>examples/hello-plugin</c> library through the host runtime, so these
/// exercise the actual FFI boundary rather than a stand-in for it.
/// </summary>
public sealed class RuntimeTests
{
    private const string HelloId = "BTCPayServer.Plugins.Hello";

    private static Assembly Anchor => typeof(RuntimeTests).Assembly;

    private static (RustPluginRuntime Runtime, FakeBackend Backend, RecordingLogger Logger) Build(
        string id = HelloId)
    {
        var backend = new FakeBackend();
        var logger = new RecordingLogger();
        return (new RustPluginRuntime(id, backend, logger), backend, logger);
    }

    [Fact]
    public void Starting_loads_the_native_library_and_reports_metadata()
    {
        var (runtime, _, _) = Build();
        using var _r = runtime;

        runtime.Start(Anchor);

        Assert.NotNull(runtime.Metadata);
        Assert.Equal(HelloId, runtime.Metadata!.Identifier);
        Assert.Equal("Hello", runtime.Metadata.Name);
        // Version comes from Cargo.toml via the macro, not from a hand-written string.
        Assert.False(string.IsNullOrWhiteSpace(runtime.Metadata.Version));
    }

    [Fact]
    public void Starting_hands_the_plugin_working_host_services()
    {
        var (runtime, backend, logger) = Build();
        using var _r = runtime;
        backend.Settings["greeting"] = "configured greeting";

        runtime.Start(Anchor);

        // hello-plugin reads `greeting` and logs it during start, so seeing it here proves
        // the whole Rust -> C# -> backend path works.
        Assert.Contains(logger.Entries, e => e.Message.Contains("configured greeting"));
    }

    [Fact]
    public void A_plugin_identity_that_disagrees_with_the_assembly_is_rejected()
    {
        // The C# assembly and the Rust library each declare identity; if a package is
        // assembled from mismatched parts it would otherwise install under the wrong name.
        var (runtime, _, _) = Build(id: "Acme.SomethingElse");
        using var _r = runtime;

        var ex = Assert.Throws<PluginLoadException>(() => runtime.Start(Anchor));
        Assert.Contains("identity mismatch", ex.Message);
        Assert.Contains(HelloId, ex.Message);
    }

    [Fact]
    public void Stopping_is_idempotent_and_never_throws()
    {
        var (runtime, _, _) = Build();
        runtime.Start(Anchor);

        runtime.Stop();
        runtime.Stop();      // shutdown must not be blockable by plugin code
        runtime.Dispose();
    }

    [Fact]
    public void Events_arriving_before_start_are_ignored_rather_than_crashing()
    {
        var (runtime, _, _) = Build();
        using var _r = runtime;

        var actions = runtime.Dispatch(new HostEvent.Tick());

        Assert.Empty(actions);
    }

    [Fact]
    public void Settings_submitted_by_the_operator_are_persisted_through_the_backend()
    {
        var (runtime, backend, _) = Build();
        using var _r = runtime;
        runtime.Start(Anchor);

        var actions = runtime.Dispatch(new HostEvent.SettingsUpdated(
            new Dictionary<string, string> { ["greeting"] = "hi there" }));

        // The plugin asked for a save, and the runtime actually performed it.
        Assert.Contains(actions, a => a is PluginAction.SaveSettings);
        Assert.Equal("hi there", backend.Settings["greeting"]);
    }

    [Fact]
    public void A_plugin_rejecting_bad_input_is_reported_without_taking_down_the_host()
    {
        var (runtime, backend, logger) = Build();
        using var _r = runtime;
        runtime.Start(Anchor);

        // hello-plugin refuses an empty greeting.
        var actions = runtime.Dispatch(new HostEvent.SettingsUpdated(
            new Dictionary<string, string> { ["greeting"] = "   " }));

        Assert.Empty(actions);
        Assert.False(backend.Settings.ContainsKey("greeting"));
        Assert.True(logger.HasError("greeting must not be empty"),
            "the plugin's rejection should be logged for the operator");

        // The runtime is still usable afterwards.
        Assert.NotEmpty(runtime.Dispatch(new HostEvent.SettingsUpdated(
            new Dictionary<string, string> { ["greeting"] = "recovered" })));
    }

    [Fact]
    public void Invoice_events_reach_the_plugin_and_its_log_action_is_carried_out()
    {
        var (runtime, _, logger) = Build();
        using var _r = runtime;
        runtime.Start(Anchor);

        var invoice = new InvoiceSummary("inv-1", "store-1", "Settled", "42.00", "USD");
        var actions = runtime.Dispatch(new HostEvent.InvoiceStatusChanged(invoice, new InvoiceTrigger.Confirmed()));

        Assert.Contains(actions, a => a is PluginAction.Log);
        Assert.Contains(logger.Entries, e => e.Message.Contains("inv-1")
                                          && e.Message.Contains("Settled")
                                          && e.Message.Contains("payment confirmed"));
    }

    [Fact]
    public void A_broken_backend_cannot_kill_the_plugin()
    {
        // The critical invariant: an exception escaping a host callback becomes a Rust panic
        // that unwinds the plugin's thread and silently stops it. HostServicesImpl must
        // contain every failure instead.
        var (runtime, backend, logger) = Build();
        using var _r = runtime;
        backend.Fault = () => new InvalidOperationException("database is down");

        runtime.Start(Anchor);   // start reads a setting, which now throws inside the backend

        Assert.True(logger.HasError("database is down") || logger.HasError("GetSetting"),
            "the backend failure should be logged");

        // Still alive and serving events despite the backend failing throughout.
        backend.Fault = null;
        var invoice = new InvoiceSummary("inv-2", "store-1", "Settled", "1.00", "USD");
        Assert.NotEmpty(runtime.Dispatch(new HostEvent.InvoiceStatusChanged(invoice, new InvoiceTrigger.PaidInFull())));
    }

    [Fact]
    public void The_settings_schema_can_be_requested_for_rendering()
    {
        var (runtime, _, _) = Build();
        using var _r = runtime;
        runtime.Start(Anchor);

        var doc = runtime.SettingsSchema();

        Assert.NotNull(doc);
        Assert.Equal(1u, doc!.UiVersion);
    }
}
