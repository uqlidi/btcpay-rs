using System.Text.Json;

namespace BtcpayRs.Host;

/// <summary>
/// A page described by a plugin, parsed from the JSON it sends.
/// </summary>
/// <remarks>
/// Parsed by hand from <see cref="JsonElement"/> rather than deserialised into a closed type
/// hierarchy, so that a section or field kind this host does not know becomes
/// <see cref="UiSection.Unknown"/> instead of failing the whole page. A plugin built against
/// a newer btcpay-rs then renders what this host understands and says so about the rest.
/// </remarks>
public sealed record UiPage(int WireVersion, string Title, IReadOnlyList<UiSection> Sections)
{
    /// <summary>Wire format this host understands.</summary>
    public const int SupportedWireVersion = 1;

    /// <summary>An empty page, used when a plugin exposes no settings.</summary>
    public static UiPage Empty { get; } = new(SupportedWireVersion, string.Empty, []);

    /// <summary>Whether the page came from a wire format newer than this host knows.</summary>
    public bool IsNewerThanSupported => WireVersion > SupportedWireVersion;

    /// <summary>Parses a document produced by <c>btcpay_ui::Document::to_json</c>.</summary>
    public static UiPage Parse(string json)
    {
        if (string.IsNullOrWhiteSpace(json)) return Empty;

        using var document = JsonDocument.Parse(json);
        var root = document.RootElement;

        var sections = new List<UiSection>();
        if (root.TryGetProperty("sections", out var array) && array.ValueKind == JsonValueKind.Array)
        {
            foreach (var element in array.EnumerateArray())
                sections.Add(ParseSection(element));
        }

        return new UiPage(
            root.TryGetProperty("wireVersion", out var v) ? v.GetInt32() : SupportedWireVersion,
            root.TryGetProperty("title", out var t) ? t.GetString() ?? string.Empty : string.Empty,
            sections);
    }

    /// <summary>Finds a form by id, to check a submission against it.</summary>
    public UiSection.Form? FormById(string formId) =>
        Sections.OfType<UiSection.Form>().FirstOrDefault(f => f.Id == formId);

    /// <summary>
    /// Finds a button by command id.
    /// </summary>
    /// <remarks>
    /// The page is rebuilt from the plugin before a press is acted on, so a command that is
    /// not on it means a stale page or a crafted post. That is also where the button's
    /// confirmation requirement is read from, rather than trusting the request.
    /// </remarks>
    public UiButton? ButtonByCommand(string command) =>
        Sections.OfType<UiSection.Actions>()
            .SelectMany(a => a.Buttons)
            .FirstOrDefault(b => b.Command == command);

    private static UiSection ParseSection(JsonElement element)
    {
        var type = Text(element, "type");
        return type switch
        {
            "form" => new UiSection.Form(
                Text(element, "id"),
                Optional(element, "title"),
                ParseFields(element),
                Text(element, "submitLabel", "Save")),
            "table" => new UiSection.Table(
                Optional(element, "title"),
                Strings(element, "columns"),
                Rows(element),
                Optional(element, "emptyMessage")),
            "stats" => new UiSection.Stats(ParseCards(element)),
            "actions" => new UiSection.Actions(Optional(element, "title"), ParseButtons(element)),
            "alert" => new UiSection.Alert(Text(element, "level", "info"), Text(element, "text")),
            "text" => new UiSection.Text(Text(element, "text")),
            _ => new UiSection.Unknown(type),
        };
    }

    private static IReadOnlyList<UiField> ParseFields(JsonElement element)
    {
        if (!element.TryGetProperty("fields", out var array) || array.ValueKind != JsonValueKind.Array)
            return [];

        return array.EnumerateArray().Select(f => new UiField(
            Text(f, "id"),
            Text(f, "label"),
            Optional(f, "help"),
            f.TryGetProperty("required", out var r) && r.ValueKind == JsonValueKind.True,
            Optional(f, "value"),
            // Field kinds are flattened onto the field, not nested in a second object.
            Text(f, "kind", "text"),
            Optional(f, "placeholder"),
            Number(f, "min"),
            Number(f, "max"),
            ParseOptions(f))).ToList();
    }

    private static IReadOnlyList<UiSelectOption> ParseOptions(JsonElement field)
    {
        if (!field.TryGetProperty("options", out var array) || array.ValueKind != JsonValueKind.Array)
            return [];

        return array.EnumerateArray()
            .Select(o => new UiSelectOption(Text(o, "value"), Text(o, "label")))
            .ToList();
    }

    private static IReadOnlyList<UiButton> ParseButtons(JsonElement element)
    {
        if (!element.TryGetProperty("buttons", out var array) || array.ValueKind != JsonValueKind.Array)
            return [];

        return array.EnumerateArray()
            .Select(b => new UiButton(
                Text(b, "command"),
                Text(b, "label"),
                Text(b, "style", "secondary"),
                Optional(b, "confirm")))
            .ToList();
    }

    private static IReadOnlyList<UiStatCard> ParseCards(JsonElement element)
    {
        if (!element.TryGetProperty("cards", out var array) || array.ValueKind != JsonValueKind.Array)
            return [];

        return array.EnumerateArray()
            .Select(c => new UiStatCard(Text(c, "label"), Text(c, "value"), Optional(c, "detail")))
            .ToList();
    }

    private static IReadOnlyList<IReadOnlyList<string>> Rows(JsonElement element)
    {
        if (!element.TryGetProperty("rows", out var array) || array.ValueKind != JsonValueKind.Array)
            return [];

        return array.EnumerateArray()
            .Select(row => (IReadOnlyList<string>)row.EnumerateArray()
                .Select(cell => cell.GetString() ?? string.Empty).ToList())
            .ToList();
    }

    private static IReadOnlyList<string> Strings(JsonElement element, string name)
    {
        if (!element.TryGetProperty(name, out var array) || array.ValueKind != JsonValueKind.Array)
            return [];

        return array.EnumerateArray().Select(e => e.GetString() ?? string.Empty).ToList();
    }

    private static string Text(JsonElement element, string name, string fallback = "") =>
        element.TryGetProperty(name, out var value) ? value.GetString() ?? fallback : fallback;

    private static string? Optional(JsonElement element, string name) =>
        element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;

    private static long? Number(JsonElement element, string name) =>
        element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.Number
            ? value.GetInt64()
            : null;
}

/// <summary>A block on a page.</summary>
public abstract record UiSection
{
    /// <summary>Inputs the operator can edit and submit.</summary>
    public sealed record Form(
        string Id,
        string? Title,
        IReadOnlyList<UiField> Fields,
        string SubmitLabel) : UiSection;

    /// <summary>Rows of read-only data.</summary>
    public sealed record Table(
        string? Title,
        IReadOnlyList<string> Columns,
        IReadOnlyList<IReadOnlyList<string>> Rows,
        string? EmptyMessage) : UiSection;

    /// <summary>A row of headline numbers.</summary>
    public sealed record Stats(IReadOnlyList<UiStatCard> Cards) : UiSection;

    /// <summary>A coloured notice.</summary>
    public sealed record Alert(string Level, string Message) : UiSection;

    /// <summary>Buttons that ask the plugin to do something.</summary>
    public sealed record Actions(string? Title, IReadOnlyList<UiButton> Buttons) : UiSection;

    /// <summary>A paragraph.</summary>
    public sealed record Text(string Value) : UiSection;

    /// <summary>
    /// A section this host does not know how to render, kept so the rest of the page still
    /// works and the operator is told why something is missing.
    /// </summary>
    public sealed record Unknown(string Type) : UiSection;
}

/// <summary>One input on a form.</summary>
public sealed record UiField(
    string Id,
    string Label,
    string? Help,
    bool Required,
    string? Value,
    string Kind,
    string? Placeholder,
    long? Min,
    long? Max,
    IReadOnlyList<UiSelectOption> Options)
{
    /// <summary>
    /// Whether this holds something that must not be shown in the page.
    /// </summary>
    /// <remarks>
    /// Rust never sends a value for these, so there is nothing to render. Submitting one
    /// empty means "keep what is stored", because the browser was never given the current
    /// value and so cannot send it back.
    /// </remarks>
    public bool IsSecret => Kind == "password";

    /// <summary>Whether the field is currently on, for a toggle.</summary>
    public bool IsOn => string.Equals(Value, "true", StringComparison.OrdinalIgnoreCase);
}

/// <summary>A button that asks the plugin to do something.</summary>
/// <param name="Command">Sent to the plugin when pressed.</param>
/// <param name="Label">Text on the button.</param>
/// <param name="Style">One of <c>primary</c>, <c>secondary</c> or <c>danger</c>.</param>
/// <param name="Confirm">
/// When set, the operator must confirm first. Enforced by the host, so a plugin cannot be
/// surprised by an unconfirmed press.
/// </param>
public sealed record UiButton(string Command, string Label, string Style, string? Confirm)
{
    /// <summary>Whether pressing this needs confirmation.</summary>
    public bool NeedsConfirmation => !string.IsNullOrEmpty(Confirm);
}

/// <summary>One choice in a dropdown.</summary>
public sealed record UiSelectOption(string Value, string Label);

/// <summary>One headline number.</summary>
public sealed record UiStatCard(string Label, string Value, string? Detail);
