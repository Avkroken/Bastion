using System.Reflection;
using System.Runtime.ExceptionServices;
using Renci.SshNet;

namespace Bastion.Core;

/// <summary>
/// Smal adapter för SSH.NET 2026.0.0:s svarsbärande kanal-keepalive.
///
/// SSH.NET har redan rätt protokollmekanism internt:
/// IChannelSession.SendKeepAliveRequest() skickar keepalive@openssh.com med
/// want-reply och väntar på SSH_MSG_CHANNEL_SUCCESS/FAILURE. ShellStream
/// exponerar däremot inte kanalen publikt. Vi kapslar därför den enda
/// reflektionspunkten här och låser kontraktet med integrationstest mot riktig
/// OpenSSH. Om en framtida SSH.NET-version ändrar internlayouten fallerar
/// adaptern tydligt i stället för att tyst degradera till en osäker sond.
/// </summary>
internal static class SshNetChannelLivenessProbe
{
    private static readonly FieldInfo ChannelField =
        typeof(ShellStream).GetField("_channel", BindingFlags.Instance | BindingFlags.NonPublic)
        ?? throw new NotSupportedException("SSH.NET ShellStream saknar det förväntade _channel-fältet");

    private static readonly MethodInfo SendKeepAliveRequest =
        ChannelField.FieldType.GetMethod("SendKeepAliveRequest", BindingFlags.Instance | BindingFlags.Public)
        ?? throw new NotSupportedException("SSH.NET-kanalen saknar SendKeepAliveRequest");

    static SshNetChannelLivenessProbe()
    {
        if (SendKeepAliveRequest.ReturnType != typeof(bool) || SendKeepAliveRequest.GetParameters().Length != 0)
        {
            throw new NotSupportedException("SSH.NET SendKeepAliveRequest har ändrat signatur");
        }
    }

    /// <summary>Forcerar statisk kontraktsvalidering innan bakgrundsbevakningen startas.</summary>
    public static void ValidateContract() => _ = SendKeepAliveRequest;

    /// <summary>
    /// Skickar en no-op kanal-request och väntar på success/failure-svar.
    /// Både true och false bevisar att servern svarade; endast undantag betyder
    /// att något transport-/timeoutfel inträffade.
    /// </summary>
    public static void SendAndWaitForReply(ShellStream shell)
    {
        ArgumentNullException.ThrowIfNull(shell);
        var channel = ChannelField.GetValue(shell)
            ?? throw new InvalidOperationException("SSH.NET ShellStream saknar en aktiv kanal");

        try
        {
            _ = SendKeepAliveRequest.Invoke(channel, parameters: null);
        }
        catch (TargetInvocationException ex) when (ex.InnerException is not null)
        {
            ExceptionDispatchInfo.Capture(ex.InnerException).Throw();
            throw;
        }
    }
}
