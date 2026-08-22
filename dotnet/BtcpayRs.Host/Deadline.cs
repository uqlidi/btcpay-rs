using Microsoft.Extensions.Logging;

namespace BtcpayRs.Host;

/// <summary>
/// Runs work that must not be allowed to block shutdown for ever.
/// </summary>
/// <remarks>
/// <para>
/// A plugin stopping is allowed to take time: draining in-flight work is the responsible thing
/// for a plugin holding funds to do. A plugin that deadlocks is a different matter, and without
/// a bound it makes BTCPay unstoppable.
/// </para>
/// <para>
/// Abandoned, never killed. There is no safe way to interrupt Rust mid-operation: aborting a
/// thread holding a lock or half-way through writing a file leaves state worse than leaving it
/// alone. The process is exiting anyway, so the work stops when it does.
/// </para>
/// </remarks>
public static class Deadline
{
    /// <summary>
    /// Runs <paramref name="work"/>, waiting at most <paramref name="timeout"/> for it.
    /// </summary>
    /// <returns>
    /// <c>true</c> when the work finished in time, <c>false</c> when it was abandoned.
    /// </returns>
    public static async Task<bool> RunAsync(
        Action work,
        TimeSpan timeout,
        ILogger logger,
        string what)
    {
        // CancellationToken.None deliberately: the work cannot be cancelled, so passing a
        // token would only make the wait itself abortable and leave the work running with
        // nobody watching it.
        var running = Task.Run(work, CancellationToken.None);

        if (await Task.WhenAny(running, Task.Delay(timeout, CancellationToken.None)) == running)
        {
            // Surface a failure in the work itself, which would otherwise be swallowed by the
            // task and look like success.
            if (running.IsFaulted)
            {
                logger.LogError(running.Exception, "{What} failed", what);
            }
            return true;
        }

        logger.LogWarning(
            "{What} did not finish within {Seconds}s and is being abandoned. Work it had not "
                + "completed may be unfinished; it should recover when it next starts.",
            what,
            timeout.TotalSeconds);
        return false;
    }
}
