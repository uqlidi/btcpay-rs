using BtcpayRs.Host;
using BTCPayServer.Abstractions.Extensions;
using BTCPayServer.Abstractions.Constants;
using BTCPayServer.Abstractions.Models;
using Microsoft.AspNetCore.Mvc;

namespace BtcpayRs.Host.BTCPay;

/// <summary>
/// Serves a plugin's settings page: renders whatever the plugin describes, and feeds a
/// submission back to it.
/// </summary>
/// <remarks>
/// Abstract, and subclassed by a controller that <c>cargo btcpay</c> generates into each
/// plugin. MVC matches on route templates, so a single shared controller would collide as
/// soon as two btcpay-rs plugins were installed together. Giving each plugin its own literal
/// route removes the ambiguity, and the generated subclass is three lines.
/// </remarks>
public abstract class RustPluginSettingsControllerBase : Controller
{
    private readonly RustPluginHostedService _plugin;

    /// <summary>Creates the controller for one plugin.</summary>
    protected RustPluginSettingsControllerBase(RustPluginHostedService plugin)
    {
        _plugin = plugin;
    }

    /// <summary>Renders the settings page.</summary>
    /// <remarks>
    /// A controller is generated for every plugin, but plenty of plugins have nothing to
    /// configure. Rather than serving them an empty page, the route behaves as though it
    /// does not exist until the plugin describes something to show.
    /// </remarks>
    [HttpGet("")]
    public IActionResult Index()
    {
        var model = BuildModel();
        if (model.Page.Sections.Count == 0) return NotFound();

        return View("~/Views/BtcpayRsSettings/Index.cshtml", model);
    }

    /// <summary>Accepts a submission and hands it to the plugin.</summary>
    [HttpPost("")]
    [ValidateAntiForgeryToken]
    public IActionResult Index(string formId)
    {
        var model = BuildModel();
        if (model.Page.Sections.Count == 0) return NotFound();

        var form = model.Page.FormById(formId ?? string.Empty);
        if (form is null)
        {
            // The page is rebuilt from the plugin on every request, so a form id that is not
            // in it means a stale page or a tampered post.
            TempData[WellKnownTempData.ErrorMessage] = "That form is no longer part of this page.";
            return RedirectToAction(nameof(Index));
        }

        var submitted = Collect(form);
        var errors = Validate(form, submitted);
        if (errors.Count > 0)
        {
            foreach (var error in errors) ModelState.AddModelError(string.Empty, error);
            // Re-render with what the operator typed, so nothing is lost on a mistake.
            return View("~/Views/BtcpayRsSettings/Index.cshtml", model with { Submitted = submitted });
        }

        var actions = _plugin.SubmitSettings(submitted);

        // The plugin rejects a submission by returning nothing and logging why.
        if (actions.Count == 0)
        {
            TempData[WellKnownTempData.ErrorMessage] =
                "The plugin rejected those settings. Check the server logs for details.";
            return View("~/Views/BtcpayRsSettings/Index.cshtml", model with { Submitted = submitted });
        }

        TempData[WellKnownTempData.SuccessMessage] = "Settings saved.";
        return RedirectToAction(nameof(Index));
    }

    private SettingsPageModel BuildModel()
    {
        var page = _plugin.SettingsPage();
        return new SettingsPageModel(
            _plugin.PluginIdentifier,
            _plugin.PluginName,
            page,
            new Dictionary<string, string>());
    }

    /// <summary>
    /// Reads the submitted value for each field the plugin actually declared.
    /// </summary>
    /// <remarks>
    /// Driven by the form rather than by the posted keys, so extra fields in a crafted post
    /// are ignored rather than reaching the plugin or its storage.
    /// </remarks>
    private Dictionary<string, string> Collect(UiSection.Form form)
    {
        var values = new Dictionary<string, string>();
        foreach (var field in form.Fields)
        {
            // An unchecked checkbox posts nothing at all, which means "false" rather than
            // "leave it alone".
            if (field.Kind == "toggle")
            {
                values[field.Id] = Request.Form.ContainsKey(field.Id) ? "true" : "false";
                continue;
            }

            var value = Request.Form[field.Id].ToString() ?? string.Empty;

            // An empty secret means "keep what is stored": the browser was never sent the
            // current value, so it cannot send it back.
            if (field.IsSecret && string.IsNullOrEmpty(value)) continue;

            values[field.Id] = value;
        }
        return values;
    }

    /// <summary>
    /// Checks a submission against the constraints the plugin declared.
    /// </summary>
    /// <remarks>
    /// The browser enforces the same rules, and a post can ignore all of them, so they are
    /// enforced again here before anything reaches the plugin.
    /// </remarks>
    private static List<string> Validate(UiSection.Form form, IReadOnlyDictionary<string, string> submitted)
    {
        var errors = new List<string>();
        foreach (var field in form.Fields)
        {
            var value = submitted.TryGetValue(field.Id, out var v) ? v.Trim() : string.Empty;

            if (value.Length == 0)
            {
                if (field.Required && !field.IsSecret)
                    errors.Add($"{field.Label} is required");
                continue;
            }

            switch (field.Kind)
            {
                case "number":
                    if (!long.TryParse(value, out var number))
                    {
                        errors.Add($"{field.Label} must be a whole number");
                    }
                    else
                    {
                        if (field.Min is { } min && number < min)
                            errors.Add($"{field.Label} must be at least {min}");
                        if (field.Max is { } max && number > max)
                            errors.Add($"{field.Label} must be at most {max}");
                    }
                    break;

                case "select":
                    if (!field.Options.Any(o => o.Value == value))
                        errors.Add($"{field.Label} is not one of the available options");
                    break;
            }
        }
        return errors;
    }
}

/// <summary>What the settings view renders.</summary>
/// <param name="PluginIdentifier">Identifies the plugin, used in the page's routes.</param>
/// <param name="PluginName">Shown as the page heading.</param>
/// <param name="Page">The page the plugin described.</param>
/// <param name="Submitted">
/// Values from a rejected submission, so the operator's typing survives a validation error.
/// </param>
public sealed record SettingsPageModel(
    string PluginIdentifier,
    string PluginName,
    UiPage Page,
    IReadOnlyDictionary<string, string> Submitted)
{
    /// <summary>The value to render for a field: what was typed, else what is stored.</summary>
    public string ValueFor(UiField field) =>
        Submitted.TryGetValue(field.Id, out var typed) ? typed : field.Value ?? string.Empty;

    /// <summary>Whether a toggle should render as checked.</summary>
    public bool IsOn(UiField field) =>
        string.Equals(ValueFor(field), "true", StringComparison.OrdinalIgnoreCase);
}
