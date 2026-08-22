using BtcpayRs.Host;
using BTCPayServer.Abstractions.Constants;
using BTCPayServer.Abstractions.Extensions;
using BTCPayServer.Abstractions.Models;
using Microsoft.AspNetCore.Mvc;

namespace BtcpayRs.Host.BTCPay;

/// <summary>
/// Serves a plugin's pages: renders whatever it describes, feeds submissions back to it, and
/// runs the commands its buttons ask for.
/// </summary>
/// <remarks>
/// <para>
/// Abstract, and subclassed by a controller that <c>cargo btcpay</c> generates into each
/// plugin. MVC matches on route templates, so a single shared controller would collide as soon
/// as two btcpay-rs plugins were installed together. Giving each plugin its own literal route
/// removes the ambiguity, and the generated subclass is three lines.
/// </para>
/// <para>
/// One action serves every page, with the page id as a route value. Pages come from the plugin
/// at runtime, so they cannot each have a compile-time route.
/// </para>
/// </remarks>
public abstract class RustPluginSettingsControllerBase : Controller
{
    private readonly RustPluginHostedService _plugin;

    /// <summary>Creates the controller for one plugin.</summary>
    protected RustPluginSettingsControllerBase(RustPluginHostedService plugin)
    {
        _plugin = plugin;
    }

    /// <summary>Renders one of the plugin's pages.</summary>
    [HttpGet("{page?}")]
    public IActionResult Index(string? page)
    {
        var pageId = page ?? DefaultPage();
        var model = BuildModel(pageId);

        // A page with nothing on it is indistinguishable from one that does not exist, and an
        // empty page is a worse answer than a not-found.
        if (model is null || model.Page.Sections.Count == 0) return NotFound();

        return View("~/Views/BtcpayRsSettings/Index.cshtml", model);
    }

    /// <summary>Accepts a form submission or a command press.</summary>
    [HttpPost("{page?}")]
    [ValidateAntiForgeryToken]
    public IActionResult Index(string? page, string? formId, string? command)
    {
        var pageId = page ?? DefaultPage();
        var model = BuildModel(pageId);
        if (model is null || model.Page.Sections.Count == 0) return NotFound();

        return command is { Length: > 0 }
            ? RunCommand(model, pageId, command)
            : SubmitForm(model, pageId, formId);
    }

    private IActionResult RunCommand(SettingsPageModel model, string pageId, string command)
    {
        // Read from the page the plugin just described, never from the request: that is what
        // makes a crafted post unable to invent a command or skip a confirmation.
        var button = model.Page.ButtonByCommand(command);
        if (button is null)
        {
            TempData[WellKnownTempData.ErrorMessage] =
                "That action is no longer available on this page.";
            return RedirectToPage(pageId);
        }

        if (button.NeedsConfirmation && Request.Form["confirmed"] != "true")
        {
            // Reached only by a post that skipped the browser's confirmation.
            TempData[WellKnownTempData.ErrorMessage] = "That action needs confirming first.";
            return RedirectToPage(pageId);
        }

        var actions = _plugin.InvokeCommand(command, pageId);
        ApplyMessages(actions);

        // A command that reported nothing still succeeded; saying so beats silence.
        if (!actions.Any(a => a is uniffi.btcpay.PluginAction.ShowMessage))
        {
            TempData[WellKnownTempData.SuccessMessage] = $"{button.Label} done.";
        }

        return RedirectToPage(pageId);
    }

    private IActionResult SubmitForm(SettingsPageModel model, string pageId, string? formId)
    {
        var form = model.Page.FormById(formId ?? string.Empty);
        if (form is null)
        {
            // The page is rebuilt on every request, so a form id that is not on it means a
            // stale page or a tampered post.
            TempData[WellKnownTempData.ErrorMessage] = "That form is no longer part of this page.";
            return RedirectToPage(pageId);
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
        ApplyMessages(actions);

        // The plugin rejects a submission by returning nothing and logging why.
        if (actions.Count == 0)
        {
            TempData[WellKnownTempData.ErrorMessage] =
                "The plugin rejected those settings. Check the server logs for details.";
            return View("~/Views/BtcpayRsSettings/Index.cshtml", model with { Submitted = submitted });
        }

        if (!actions.Any(a => a is uniffi.btcpay.PluginAction.ShowMessage))
        {
            TempData[WellKnownTempData.SuccessMessage] = "Settings saved.";
        }

        return RedirectToPage(pageId);
    }

    /// <summary>Surfaces whatever the plugin asked to tell the operator.</summary>
    private void ApplyMessages(IReadOnlyList<uniffi.btcpay.PluginAction> actions)
    {
        foreach (var action in actions.OfType<uniffi.btcpay.PluginAction.ShowMessage>())
        {
            var key = action.Level switch
            {
                uniffi.btcpay.MessageLevel.Error or uniffi.btcpay.MessageLevel.Warning =>
                    WellKnownTempData.ErrorMessage,
                _ => WellKnownTempData.SuccessMessage,
            };
            TempData[key] = action.Text;
        }
    }

    private IActionResult RedirectToPage(string pageId) =>
        RedirectToAction(nameof(Index), new { page = pageId });

    /// <summary>The page shown when no id is given: the first the plugin offers.</summary>
    private string DefaultPage() => _plugin.Pages().FirstOrDefault()?.Id ?? "settings";

    private SettingsPageModel? BuildModel(string pageId)
    {
        var page = _plugin.PageDocument(pageId);
        if (page is null) return null;

        return new SettingsPageModel(
            _plugin.PluginIdentifier,
            _plugin.PluginName,
            pageId,
            page,
            new Dictionary<string, string>());
    }

    /// <summary>
    /// Reads the submitted value for each field the plugin actually declared.
    /// </summary>
    /// <remarks>
    /// Driven by the form rather than by the posted keys, so extra fields in a crafted post are
    /// ignored rather than reaching the plugin or its storage.
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
    /// The browser enforces the same rules and a post can ignore all of them, so they are
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

/// <summary>What the page view renders.</summary>
/// <param name="PluginIdentifier">Identifies the plugin.</param>
/// <param name="PluginName">Fallback heading when the page has no title.</param>
/// <param name="PageId">Which page this is, used when posting back to it.</param>
/// <param name="Page">The page the plugin described.</param>
/// <param name="Submitted">
/// Values from a rejected submission, so the operator's typing survives a validation error.
/// </param>
public sealed record SettingsPageModel(
    string PluginIdentifier,
    string PluginName,
    string PageId,
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
