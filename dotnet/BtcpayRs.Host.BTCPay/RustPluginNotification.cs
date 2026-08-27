using BTCPayServer.Abstractions.Contracts;
using BTCPayServer.Services.Notifications;

namespace BtcpayRs.Host.BTCPay;

/// <summary>A notification raised by a Rust plugin.</summary>
public class RustPluginNotification : BaseNotification
{
    /// <summary>Which plugin raised it.</summary>
    public string PluginId { get; set; } = string.Empty;

    /// <summary>Short headline.</summary>
    public string Title { get; set; } = string.Empty;

    /// <summary>Body text.</summary>
    public string Body { get; set; } = string.Empty;

    /// <summary>Optional BTCPay-relative link.</summary>
    public string? Link { get; set; }

    /// <summary>The type BTCPay looks the handler up by.</summary>
    public static string TypeFor(string pluginId) => $"btcpay-rs:{pluginId}";

    /// <inheritdoc />
    public override string Identifier => TypeFor(PluginId);

    /// <inheritdoc />
    public override string NotificationType => TypeFor(PluginId);

    /// <summary>Renders the notification for BTCPay's list.</summary>
    public class Handler : NotificationHandler<RustPluginNotification>
    {
        private readonly string _pluginId;
        private readonly string _pluginName;

        /// <summary>Builds a handler for one plugin.</summary>
        public Handler(string pluginId, string pluginName)
        {
            _pluginId = pluginId;
            _pluginName = pluginName;
        }

        /// <inheritdoc />
        public override string NotificationType => TypeFor(_pluginId);

        /// <inheritdoc />
        public override (string identifier, string name)[] Meta => [(TypeFor(_pluginId), _pluginName)];

        /// <inheritdoc />
        protected override void FillViewModel(RustPluginNotification notification, NotificationViewModel vm)
        {
            vm.Identifier = notification.Identifier;
            vm.Type = notification.NotificationType;
            vm.Body = string.IsNullOrWhiteSpace(notification.Body)
                ? notification.Title
                : $"{notification.Title}: {notification.Body}";
            vm.ActionLink = notification.Link;
        }
    }
}
