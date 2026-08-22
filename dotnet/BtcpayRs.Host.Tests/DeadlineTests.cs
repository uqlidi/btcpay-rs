using Xunit;

namespace BtcpayRs.Host.Tests;

/// <summary>
/// The bound on plugin shutdown. Without it a plugin that deadlocks makes BTCPay unstoppable,
/// which is the failure this exists to prevent.
/// </summary>
public sealed class DeadlineTests
{
    [Fact]
    public async Task Work_that_finishes_in_time_is_reported_as_finished()
    {
        var logger = new RecordingLogger();
        var ran = false;

        var finished = await Deadline.RunAsync(
            () => ran = true, TimeSpan.FromSeconds(5), logger, "stopping");

        Assert.True(finished);
        Assert.True(ran);
        Assert.Empty(logger.Entries);
    }

    [Fact]
    public async Task Work_that_hangs_is_abandoned_rather_than_waited_on()
    {
        // The whole point: a plugin that never returns must not hold shutdown open.
        var logger = new RecordingLogger();
        var release = new ManualResetEventSlim(false);

        var started = DateTime.UtcNow;
        var finished = await Deadline.RunAsync(
            () => release.Wait(TimeSpan.FromMinutes(1)),
            TimeSpan.FromMilliseconds(200),
            logger,
            "stopping");
        var waited = DateTime.UtcNow - started;

        Assert.False(finished);
        Assert.True(waited < TimeSpan.FromSeconds(5),
            $"should have given up promptly, waited {waited.TotalSeconds:F1}s");

        // The operator has to be able to tell this happened, otherwise it is a silent hang.
        Assert.Contains(logger.Entries, e => e.Message.Contains("abandoned"));

        release.Set();
    }

    [Fact]
    public async Task Work_that_throws_is_reported_rather_than_swallowed()
    {
        // Task.Run captures the exception; without this it would look like a clean stop.
        var logger = new RecordingLogger();

        var finished = await Deadline.RunAsync(
            () => throw new InvalidOperationException("stop blew up"),
            TimeSpan.FromSeconds(5),
            logger,
            "stopping");

        Assert.True(finished, "it did finish, just badly");
        Assert.True(logger.HasError("stop blew up") || logger.HasError("failed"),
            "the failure should be logged");
    }
}
