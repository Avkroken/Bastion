using System.Collections.ObjectModel;
using Bastion.Core;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Renci.SshNet.Common;

namespace Bastion;

public sealed class HostRow
{
    public required Guid Id { get; init; }
    public required string Alias { get; init; }
    public required string Subtitle { get; init; }
}

public sealed partial class MainWindow : Window
{
    private readonly HostStore _store = new(HostStore.DefaultPath);
    private readonly KnownHosts _knownHosts = new(Bastion.Core.KnownHosts.DefaultPath);
    private readonly ObservableCollection<HostRow> _rows = new();
    private readonly DispatcherQueue _dispatcher = DispatcherQueue.GetForCurrentThread();
    private SshSession? _activeSession;

    public MainWindow()
    {
        InitializeComponent();
        Title = "Bastion";
        HostListView.ItemsSource = _rows;
        Refresh();
    }

    private void Refresh()
    {
        _rows.Clear();
        foreach (var h in _store.All())
        {
            _rows.Add(new HostRow
            {
                Id = h.Id,
                Alias = h.Alias,
                Subtitle = $"{h.User}@{h.HostName}:{h.Port}",
            });
        }
    }

    private async void OnAddHostClicked(object sender, RoutedEventArgs e)
    {
        var aliasBox = new TextBox { PlaceholderText = "Alias" };
        var hostBox = new TextBox { PlaceholderText = "Värdnamn/IP" };
        var userBox = new TextBox { PlaceholderText = "Användare" };
        var portBox = new TextBox { PlaceholderText = "Port", Text = "22" };

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(aliasBox);
        panel.Children.Add(hostBox);
        panel.Children.Add(userBox);
        panel.Children.Add(portBox);

        var dialog = new ContentDialog
        {
            Title = "Lägg till värd",
            Content = panel,
            PrimaryButtonText = "Lägg till",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary) return;

        var alias = aliasBox.Text.Trim();
        var hostName = hostBox.Text.Trim();
        var user = userBox.Text.Trim();
        if (alias.Length == 0 || hostName.Length == 0 || user.Length == 0) return;

        var host = Host.Create(alias, hostName, user);
        if (long.TryParse(portBox.Text, out var port)) host.Port = port;

        _store.Upsert(host);
        Refresh();
    }

    private async void OnHostItemClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not HostRow row) return;
        var host = _store.All().FirstOrDefault(h => h.Id == row.Id);
        if (host is null) return;

        string? password = null;
        if (host.Auth is HostAuth.AskPassword)
        {
            password = await PromptPasswordAsync(host);
            if (password is null) return; // avbrutet
        }

        await ConnectAndShowTerminalAsync(host, password);
    }

    private async Task<string?> PromptPasswordAsync(Host host)
    {
        var passwordBox = new PasswordBox { PlaceholderText = "Lösenord" };
        var dialog = new ContentDialog
        {
            Title = $"Lösenord för {host.User}@{host.HostName}",
            Content = passwordBox,
            PrimaryButtonText = "Anslut",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        return result == ContentDialogResult.Primary ? passwordBox.Password : null;
    }

    private async Task ConnectAndShowTerminalAsync(Host host, string? password)
    {
        _activeSession?.Dispose();
        _activeSession = null;

        ContentPlaceholder.Text = $"Ansluter till {host.Alias}…";

        await TerminalView.EnsureCoreWebView2Async();
        var htmlPath = Path.Combine(AppContext.BaseDirectory, "Assets", "xterm", "terminal.html");
        var navigated = new TaskCompletionSource();
        void OnNavigationCompleted(WebView2 s, Microsoft.Web.WebView2.Core.CoreWebView2NavigationCompletedEventArgs a) => navigated.TrySetResult();
        TerminalView.NavigationCompleted += OnNavigationCompleted;
        TerminalView.CoreWebView2.Navigate(new Uri(htmlPath).AbsoluteUri);
        await navigated.Task;
        TerminalView.NavigationCompleted -= OnNavigationCompleted;

        SshSession session;
        try
        {
            session = await Task.Run(() => SshSession.Connect(host, password, _knownHosts));
        }
        catch (SshHostKeyChangedException ex)
        {
            ContentPlaceholder.Text = ex.Message;
            PlaceholderPanel.Visibility = Visibility.Visible;
            TerminalView.Visibility = Visibility.Collapsed;
            return;
        }
        catch (Exception ex)
        {
            ContentPlaceholder.Text = $"Anslutning misslyckades: {ex.Message}";
            PlaceholderPanel.Visibility = Visibility.Visible;
            TerminalView.Visibility = Visibility.Collapsed;
            return;
        }

        _activeSession = session;
        PlaceholderPanel.Visibility = Visibility.Collapsed;
        TerminalView.Visibility = Visibility.Visible;

        TerminalView.CoreWebView2.WebMessageReceived += (_, args) =>
        {
            var text = args.TryGetWebMessageAsString();
            session.Shell.Write(text);
        };

        session.Shell.DataReceived += (_, args) => FeedTerminal(args);
        session.Shell.Closed += (_, _) => _dispatcher.TryEnqueue(() => OnSessionClosed(session));
    }

    private void FeedTerminal(ShellDataEventArgs args)
    {
        var base64 = Convert.ToBase64String(args.Data);
        _dispatcher.TryEnqueue(() =>
        {
            if (_activeSession is null) return; // sessionen stängdes redan
            _ = TerminalView.CoreWebView2.ExecuteScriptAsync($"window.feed('{base64}')");
        });
    }

    private void OnSessionClosed(SshSession session)
    {
        if (!ReferenceEquals(_activeSession, session)) return; // en nyare session har redan tagit över
        _activeSession = null;
        TerminalView.Visibility = Visibility.Collapsed;
        PlaceholderPanel.Visibility = Visibility.Visible;
        ContentPlaceholder.Text = "Sessionen avslutades — välj en värd i listan.";
    }
}
