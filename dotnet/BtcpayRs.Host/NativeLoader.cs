using System.Reflection;
using System.Runtime.InteropServices;
using uniffi.btcpay;

namespace BtcpayRs.Host;

/// <summary>
/// Locates and loads a plugin's Rust library, and verifies it speaks an ABI this host
/// understands.
/// </summary>
/// <remarks>
/// <para>
/// Resolution is explicit rather than left to the runtime. BTCPay's plugin
/// <c>AssemblyLoadContext</c> resolves native libraries from <c>deps.json</c>, and a
/// <c>.so</c> merely copied into <c>runtimes/{rid}/native/</c> produces no <c>deps.json</c>
/// entry, so that conventional layout does not resolve on its own. A library sitting flat
/// beside the plugin assembly does resolve, via CoreCLR's probing of the declaring
/// assembly's directory, but relying on that leaves failure looking like
/// <c>TypeInitializationException</c> with no indication of what is missing.
/// </para>
/// <para>
/// Each plugin gets its own copy of this assembly inside its own load context, so the
/// resolver registered here is private to one plugin. Plugins therefore stay isolated even
/// though every plugin's native library has the same file name.
/// </para>
/// </remarks>
public static class NativeLoader
{
    /// <summary>
    /// File name (without <c>lib</c> prefix or extension) that every btcpay-rs plugin builds
    /// its cdylib under. Must match <c>[DllImport]</c> in the generated bindings.
    /// </summary>
    public const string NativeName = "btcpay_plugin_native";

    /// <summary>ABI this host understands. Must match the Rust <c>ABI_VERSION</c>.</summary>
    public const uint SupportedAbi = 3;

    private static readonly object Gate = new();
    private static bool _registered;

    /// <summary>
    /// Registers native resolution for this load context and performs the ABI handshake.
    /// Idempotent; safe to call from every entry point.
    /// </summary>
    /// <param name="pluginAssembly">
    /// The plugin assembly whose directory contains the native library.
    /// </param>
    /// <exception cref="PluginLoadException">
    /// The library is missing, unloadable, or reports an incompatible ABI.
    /// </exception>
    public static void Initialize(Assembly pluginAssembly)
    {
        lock (Gate)
        {
            if (!_registered)
            {
                NativeLibrary.SetDllImportResolver(
                    typeof(NativeLoader).Assembly,
                    (name, _, _) => name == NativeName ? Resolve(pluginAssembly) : IntPtr.Zero);
                _registered = true;
            }
        }

        VerifyAbi();
    }

    /// <summary>
    /// Confirms the loaded library's ABI matches <see cref="SupportedAbi"/>. This is the
    /// first call into Rust, so it is also where a missing library surfaces.
    /// </summary>
    private static void VerifyAbi()
    {
        uint reported;
        try
        {
            reported = BtcpayMethods.BtcpayRsAbiVersion();
        }
        catch (Exception ex)
        {
            throw new PluginLoadException(
                $"could not call into the plugin's native library '{LibraryFileName}'. " +
                "It is missing, built for a different platform, or corrupt.", ex);
        }

        if (reported != SupportedAbi)
        {
            throw new PluginLoadException(
                $"plugin ABI mismatch: the native library reports ABI {reported}, but this " +
                $"host supports ABI {SupportedAbi}. Rebuild the plugin against a matching " +
                "version of btcpay-rs.");
        }
    }

    private static string LibraryFileName => $"lib{NativeName}.so";

    private static IntPtr Resolve(Assembly pluginAssembly)
    {
        var probed = new List<string>();
        foreach (var candidate in Candidates(pluginAssembly))
        {
            probed.Add(candidate);
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var handle))
                return handle;
        }

        throw new PluginLoadException(
            $"could not locate the plugin's native library '{LibraryFileName}' for runtime " +
            $"'{RuntimeInformation.RuntimeIdentifier}'. Searched: {string.Join(", ", probed)}. " +
            "The plugin package is incomplete or was built for a different platform.");
    }

    private static IEnumerable<string> Candidates(Assembly pluginAssembly)
    {
        var dir = Path.GetDirectoryName(pluginAssembly.Location);
        if (string.IsNullOrEmpty(dir)) yield break;

        // Flat beside the plugin assembly: what the packaging step produces.
        yield return Path.Combine(dir, LibraryFileName);

        // Also honour the conventional layout, so a plugin packaged as a multi-RID NuGet
        // still works even though the ALC would not resolve it unaided.
        var rid = RuntimeInformation.RuntimeIdentifier;
        yield return Path.Combine(dir, "runtimes", rid, "native", LibraryFileName);
        if (rid != "linux-x64")
            yield return Path.Combine(dir, "runtimes", "linux-x64", "native", LibraryFileName);
    }
}

/// <summary>Thrown when a plugin's native library cannot be loaded or is incompatible.</summary>
public sealed class PluginLoadException : Exception
{
    /// <summary>Creates the exception with a message.</summary>
    public PluginLoadException(string message) : base(message) { }

    /// <summary>Creates the exception with a message and underlying cause.</summary>
    public PluginLoadException(string message, Exception inner) : base(message, inner) { }
}
