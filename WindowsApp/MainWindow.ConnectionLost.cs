using Bastion.Core;
using Microsoft.UI.Xaml.Controls;

namespace Bastion;

public sealed partial class MainWindow
{
    /// <summary>
    /// Tar emot kärnans verifierade liveness-förlust och återanvänder samma
    /// idempotenta tabbstädning som en normal remote shell-close. Sessionen
    /// identifieras via TabViewItem.Tag; en redan stängd flik ignoreras.
    /// </summary>
    internal void HandleConnectionLost(SshSession session)
    {
        _dispatcher.TryEnqueue(() =>
        {
            var tab = SessionTabView.TabItems
                .OfType<TabViewItem>()
                .FirstOrDefault(candidate => ReferenceEquals(candidate.Tag, session));
            if (tab is null) return;
            OnSessionClosed(tab, session);
        });
    }
}
