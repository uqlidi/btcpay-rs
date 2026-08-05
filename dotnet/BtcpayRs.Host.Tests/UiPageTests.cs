using Xunit;

namespace BtcpayRs.Host.Tests;

/// <summary>
/// Parsing the page a plugin describes. These are the host's half of a contract whose other
/// half lives in the btcpay-ui crate, so they use the exact JSON that crate produces.
/// </summary>
public sealed class UiPageTests
{
    [Fact]
    public void A_form_is_parsed_with_its_fields_in_order()
    {
        var page = UiPage.Parse("""
        {"wireVersion":1,"title":"Settings","sections":[
          {"type":"form","id":"main","title":"Greeting","submitLabel":"Save it","fields":[
            {"id":"greeting","label":"Greeting","required":true,"kind":"text","placeholder":"Hi"},
            {"id":"count","label":"Count","required":false,"value":"3","kind":"number","min":1,"max":10}
          ]}
        ]}
        """);

        Assert.Equal("Settings", page.Title);
        var form = Assert.IsType<UiSection.Form>(page.Sections.Single());
        Assert.Equal("main", form.Id);
        Assert.Equal("Save it", form.SubmitLabel);

        Assert.Equal("greeting", form.Fields[0].Id);
        Assert.True(form.Fields[0].Required);
        Assert.Equal("Hi", form.Fields[0].Placeholder);

        Assert.Equal("3", form.Fields[1].Value);
        Assert.Equal(1, form.Fields[1].Min);
        Assert.Equal(10, form.Fields[1].Max);
    }

    [Fact]
    public void An_unknown_section_does_not_take_down_the_rest_of_the_page()
    {
        // A plugin built against a newer btcpay-rs must still render what this host knows,
        // rather than showing a blank page or an error.
        var page = UiPage.Parse("""
        {"wireVersion":1,"title":"T","sections":[
          {"type":"text","text":"before"},
          {"type":"somethingNew","whatever":true},
          {"type":"text","text":"after"}
        ]}
        """);

        Assert.Equal(3, page.Sections.Count);
        Assert.Equal("before", Assert.IsType<UiSection.Text>(page.Sections[0]).Value);
        Assert.Equal("somethingNew", Assert.IsType<UiSection.Unknown>(page.Sections[1]).Type);
        Assert.Equal("after", Assert.IsType<UiSection.Text>(page.Sections[2]).Value);
    }

    [Fact]
    public void An_unknown_field_kind_is_kept_so_the_form_still_submits()
    {
        // Dropping it would silently discard whatever the operator had configured.
        var page = UiPage.Parse("""
        {"wireVersion":1,"title":"T","sections":[
          {"type":"form","id":"f","fields":[{"id":"x","label":"X","kind":"colourPicker"}]}
        ]}
        """);

        var form = Assert.IsType<UiSection.Form>(page.Sections.Single());
        Assert.Equal("colourPicker", form.Fields.Single().Kind);
    }

    [Fact]
    public void A_newer_wire_version_is_reported_rather_than_mis_rendered()
    {
        var page = UiPage.Parse("""{"wireVersion":99,"title":"T","sections":[]}""");

        Assert.True(page.IsNewerThanSupported);
    }

    [Fact]
    public void A_password_field_is_recognised_as_secret_and_carries_no_value()
    {
        // Rust refuses to serialise a value for these; the host must not invent one.
        var page = UiPage.Parse("""
        {"wireVersion":1,"title":"T","sections":[
          {"type":"form","id":"f","fields":[{"id":"k","label":"Key","kind":"password"}]}
        ]}
        """);

        var field = Assert.IsType<UiSection.Form>(page.Sections.Single()).Fields.Single();
        Assert.True(field.IsSecret);
        Assert.Null(field.Value);
    }

    [Fact]
    public void Tables_stats_and_alerts_are_parsed()
    {
        var page = UiPage.Parse("""
        {"wireVersion":1,"title":"T","sections":[
          {"type":"table","columns":["A","B"],"rows":[["1","2"]],"emptyMessage":"none"},
          {"type":"stats","cards":[{"label":"Swaps","value":"12","detail":"today"}]},
          {"type":"alert","level":"warning","text":"Careful"}
        ]}
        """);

        var table = Assert.IsType<UiSection.Table>(page.Sections[0]);
        Assert.Equal(["A", "B"], table.Columns);
        Assert.Equal("1", table.Rows.Single()[0]);
        Assert.Equal("none", table.EmptyMessage);

        var card = Assert.IsType<UiSection.Stats>(page.Sections[1]).Cards.Single();
        Assert.Equal("Swaps", card.Label);
        Assert.Equal("today", card.Detail);

        var alert = Assert.IsType<UiSection.Alert>(page.Sections[2]);
        Assert.Equal("warning", alert.Level);
    }

    [Fact]
    public void An_empty_or_missing_document_yields_an_empty_page_rather_than_throwing()
    {
        Assert.Empty(UiPage.Parse("").Sections);
        Assert.Empty(UiPage.Parse("   ").Sections);
        Assert.Empty(UiPage.Parse("""{"wireVersion":1,"title":"T"}""").Sections);
    }

    [Fact]
    public void A_form_can_be_found_by_id_to_validate_a_submission()
    {
        var page = UiPage.Parse("""
        {"wireVersion":1,"title":"T","sections":[{"type":"form","id":"main","fields":[]}]}
        """);

        Assert.NotNull(page.FormById("main"));
        Assert.Null(page.FormById("other"));
    }
}
