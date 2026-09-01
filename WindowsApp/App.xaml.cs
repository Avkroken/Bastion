using Bastion.Core;
using Microsoft.UI.Xaml;

namespace Bastion;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        InitializeComponent();
        SshSession.SessionConnectionLost += OnSessionConnectionLost;
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        _window = new MainWindow();
        _window.Activate();
    }

    private void OnSessionConnectionLost(SshSession session)
    {
        if (_window is MainWindow mainWindow)
        {
            mainWindow.HandleConnectionLost(session);
        }
    }
}
