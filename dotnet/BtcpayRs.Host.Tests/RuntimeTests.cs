using System.Reflection;
using System.Text.Json;
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
    public void A_plugin_can_write_files_in_its_data_directory()
    {
        // Not just that a path is handed over: that a plugin writing to it actually succeeds.
        // hello-plugin writes a file during start.
        var (runtime, backend, logger) = Build();
        using var _r = runtime;
        using var _b = backend;

        runtime.Start(Anchor);

        var written = Path.Combine(backend.DataDirectory, "last-start.txt");
        Assert.True(File.Exists(written),
            $"the plugin should have written into {backend.DataDirectory}");
        Assert.False(logger.HasError("could not write"),
            "writing should not have failed");
    }

    [Fact]
    public void Changing_a_setting_takes_effect_on_the_running_plugin()
    {
        // Persisting a value is not enough: a plugin that only reads its settings at startup
        // would look saved while continuing to use the old value until BTCPay restarts.
        var (runtime, backend, logger) = Build();
        using var _r = runtime;
        backend.Settings["greeting"] = "before";
        runtime.Start(Anchor);

        var invoice = new InvoiceSummary("inv-1", "store-1", "Settled", "1.00", "USD");
        runtime.Dispatch(new HostEvent.InvoiceStatusChanged(invoice, new InvoiceTrigger.Confirmed()));
        Assert.Contains(logger.Entries, e => e.Message.Contains("before: invoice inv-1"));

        runtime.Dispatch(new HostEvent.SettingsUpdated(
            new Dictionary<string, string> { ["greeting"] = "after" }));

        // The same event now reports the new greeting, with no restart in between.
        runtime.Dispatch(new HostEvent.InvoiceStatusChanged(invoice, new InvoiceTrigger.Confirmed()));
        Assert.Contains(logger.Entries, e => e.Message.Contains("after: invoice inv-1"));
        Assert.Equal("after", backend.Settings["greeting"]);
    }

    [Fact]
    public void A_rejected_settings_change_leaves_the_running_plugin_alone()
    {
        // A submission the plugin refuses must not half-apply: neither stored nor in effect.
        var (runtime, backend, logger) = Build();
        using var _r = runtime;
        backend.Settings["greeting"] = "kept";
        runtime.Start(Anchor);

        runtime.Dispatch(new HostEvent.SettingsUpdated(
            new Dictionary<string, string> { ["greeting"] = "   " }));

        Assert.Equal("kept", backend.Settings["greeting"]);

        var invoice = new InvoiceSummary("inv-2", "store-1", "Settled", "1.00", "USD");
        runtime.Dispatch(new HostEvent.InvoiceStatusChanged(invoice, new InvoiceTrigger.Confirmed()));
        Assert.Contains(logger.Entries, e => e.Message.Contains("kept: invoice inv-2"));
    }

    [Fact]
    public void A_saved_setting_is_reflected_in_the_form_the_operator_sees_next()
    {
        var (runtime, _, _) = Build();
        using var _r = runtime;
        runtime.Start(Anchor);

        runtime.Dispatch(new HostEvent.SettingsUpdated(
            new Dictionary<string, string> { ["greeting"] = "round tripped" }));

        var page = UiPage.Parse(runtime.SettingsSchema()!.DocumentJson);
        var greeting = page.FormById("settings")!.Fields.First(f => f.Id == "greeting");
        Assert.Equal("round tripped", greeting.Value);
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
        Assert.Equal("Hello", doc.Title);
    }

    [Fact]
    public void The_settings_schema_carries_a_page_the_host_can_render()
    {
        // The page crosses the boundary as JSON rather than generated types, so this is
        // where a mismatch between what Rust writes and what the host expects would show up.
        var (runtime, _, _) = Build();
        using var _r = runtime;
        runtime.Start(Anchor);

        using var page = JsonDocument.Parse(runtime.SettingsSchema()!.DocumentJson);
        var root = page.RootElement;

        Assert.Equal(1, root.GetProperty("wireVersion").GetInt32());

        var sections = root.GetProperty("sections").EnumerateArray().ToList();
        Assert.Contains(sections, s => s.GetProperty("type").GetString() == "form");

        var form = sections.First(s => s.GetProperty("type").GetString() == "form");
        var fields = form.GetProperty("fields").EnumerateArray().ToList();
        Assert.Equal(3, fields.Count);

        // Field kinds are flattened onto the field, not nested in a second object.
        var greeting = fields[0];
        Assert.Equal("greeting", greeting.GetProperty("id").GetString());
        Assert.Equal("text", greeting.GetProperty("kind").GetString());
        Assert.True(greeting.GetProperty("required").GetBoolean());
    }

    [Fact]
    public void Stored_settings_are_reflected_back_into_the_rendered_form()
    {
        // The operator must see what is currently configured, not an empty form.
        var (runtime, backend, _) = Build();
        using var _r = runtime;
        backend.Settings["greeting"] = "configured greeting";
        runtime.Start(Anchor);

        using var page = JsonDocument.Parse(runtime.SettingsSchema()!.DocumentJson);
        var greeting = page.RootElement
            .GetProperty("sections").EnumerateArray()
            .First(s => s.GetProperty("type").GetString() == "form")
            .GetProperty("fields").EnumerateArray()
            .First(f => f.GetProperty("id").GetString() == "greeting");

        Assert.Equal("configured greeting", greeting.GetProperty("value").GetString());
    }
}
