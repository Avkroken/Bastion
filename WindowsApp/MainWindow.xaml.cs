using System.Collections.ObjectModel;
using Bastion.Core;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using Renci.SshNet.Common;
using System.Text;
using Windows.Storage.Pickers;
using Windows.System;
using WinRT.Interop;

namespace Bastion;

public sealed class HostRow
{
    public required Guid Id { get; init; }
    public required string Alias { get; init; }
    public required string Subtitle { get; init; }
}

/// <summary>
/// En taggsektion i värdlistan. Grupperingen och filtreringen görs av
/// <see cref="HostGrouping"/> i kärnan (testad); det här är bara skalet
/// <see cref="CollectionViewSource"/> vill ha — <c>Title</c> för rubriken,
/// <c>Hosts</c> som <c>ItemsPath</c>.
/// </summary>
public sealed class HostSection
{
    public required string Title { get; init; }
    public required ObservableCollection<HostRow> Hosts { get; init; }
}

/// <summary>
/// En session/vy per flik i <see cref="MainWindow.SessionTabView"/> — motsvarar
/// LinuxApps <c>AdwTabView</c> (en flik per SSH-session eller Docker-vy) och
/// iOS <c>MultiSessionView</c>. Terminalflikar bär sin <see cref="SshSession"/>
/// i <see cref="TabViewItem.Tag"/> så den kan städas när fliken stängs; Docker-
/// flikar har inget beständigt Tag (varje åtgärd kör ett eget engångskommando
/// via <see cref="SshSession.RunCommand"/>, samma modell som LinuxApp/src/docker.rs).
/// </summary>
public sealed partial class MainWindow : Window
{
    private readonly HostStore _store = new(HostStore.DefaultPath);
    private readonly KnownHosts _knownHosts = new(Bastion.Core.KnownHosts.DefaultPath);
    private readonly ObservableCollection<HostSection> _sections = new();
    private readonly CollectionViewSource _hostSource = new()
    {
        IsSourceGrouped = true,
        ItemsPath = new PropertyPath("Hosts"),
    };
    // Fullt kvalificerad: både Microsoft.UI.Dispatching och Windows.System (som
    // `Launcher` behöver) har en DispatcherQueue, och CS0104 stoppade hela bygget.
    private readonly Microsoft.UI.Dispatching.DispatcherQueue _dispatcher =
        Microsoft.UI.Dispatching.DispatcherQueue.GetForCurrentThread();
    private readonly SnippetStore _snippetStore = new(SnippetStore.DefaultPath);
    private readonly AppSettingsStore _settingsStore = new();
    private SyncConfig _syncConfig = SyncConfig.Load(SyncConfig.DefaultPath);

    public MainWindow()
    {
        InitializeComponent();
        Title = "Bastion";
        _hostSource.Source = _sections;
        HostListView.ItemsSource = _hostSource.View;
        Refresh();
    }

    /// <summary>
    /// Bygger om värdlistan: taggsektioner med favoriter först, filtrerade på
    /// sökrutans text. Ordningen bestäms av <see cref="HostGrouping"/> så
    /// Windows visar samma sektioner som iOS/macOS och Linux.
    /// </summary>
    private void Refresh()
    {
        _sections.Clear();
        foreach (var group in HostGrouping.GroupedAndFiltered(_store.All(), HostSearchBox.Text ?? ""))
        {
            _sections.Add(new HostSection
            {
                Title = group.Tag,
                Hosts = new ObservableCollection<HostRow>(group.Hosts.Select(h => new HostRow
                {
                    Id = h.Id,
                    Alias = h.Alias,
                    Subtitle = $"{h.User}@{h.HostName}:{h.Port}",
                })),
            });
        }
    }

    private void OnHostSearchChanged(object sender, TextChangedEventArgs e) => Refresh();

    private Host? FindHost(Guid id) => _store.All().FirstOrDefault(h => h.Id == id);

    /// <summary>
    /// Auth-alternativ som faktiskt FUNGERAR i WindowsApp (se SshSession.cs).
    /// AgentDefault (SSH-agent) saknas medvetet — SSH.NET har inget
    /// agent-protokollstöd — så den döljs här istället för att skapa värdar
    /// som ser sparade ut men aldrig kan ansluta.
    /// </summary>
    private async void OnAddHostClicked(object sender, RoutedEventArgs e)
    {
        var aliasBox = new TextBox { PlaceholderText = "Alias" };
        var hostBox = new TextBox { PlaceholderText = "Värdnamn/IP" };
        var userBox = new TextBox { PlaceholderText = "Användare" };
        var portBox = new TextBox { PlaceholderText = "Port", Text = "22" };
        var authCombo = new ComboBox
        {
            ItemsSource = new[] { "Nyckelfil", "Lösenord (frågas vid varje anslutning)" },
            SelectedIndex = 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var keyPathBox = new TextBox { PlaceholderText = @"Sökväg till privat nyckel, t.ex. C:\Users\du\.ssh\id_ed25519" };
        authCombo.SelectionChanged += (_, _) => keyPathBox.Visibility = authCombo.SelectedIndex == 0 ? Visibility.Visible : Visibility.Collapsed;

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(aliasBox);
        panel.Children.Add(hostBox);
        panel.Children.Add(userBox);
        panel.Children.Add(portBox);
        panel.Children.Add(authCombo);
        panel.Children.Add(keyPathBox);

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
        host.Auth = authCombo.SelectedIndex == 0
            ? new HostAuth.KeyFile(keyPathBox.Text.Trim())
            : new HostAuth.AskPassword();

        _store.Upsert(host);
        Refresh();
    }

    /// <summary>
    /// Port av samma synk-UI som LinuxApp: välj en mapp (synkad av något
    /// annat, t.ex. Syncthing/en klonad Git-mapp) + kör HostStore.Sync mot
    /// en FolderSyncProvider där. Biblioteksnivån (Bastion.Core.SyncEngine/
    /// FolderSyncProvider) är redan verifierad (cross-instans-
    /// konvergenstestet, delat med LinuxApp/src/sync.rs) — detta kopplar
    /// bara in ytan.
    /// </summary>
    private async void OnSyncClicked(object sender, RoutedEventArgs e)
    {
        var pathLabel = new TextBlock
        {
            Text = _syncConfig.FolderPath ?? "Ingen mapp vald",
            Opacity = 0.7,
            TextWrapping = TextWrapping.Wrap,
        };
        var chooseButton = new Button { Content = "Välj mapp…" };
        var encryptedToggle = new ToggleSwitch
        {
            Header = "Kryptera (för molnmappar du inte litar på blint)",
            OnContent = "Dropbox/Drive/OneDrive — AES-256-GCM",
            OffContent = "Okrypterad lokal mapp",
            IsOn = _syncConfig.Encrypted,
        };
        var passphraseBox = new PasswordBox
        {
            PlaceholderText = "Lösenfras",
            Visibility = _syncConfig.Encrypted ? Visibility.Visible : Visibility.Collapsed,
        };
        var syncButton = new Button { Content = "Synka nu", IsEnabled = _syncConfig.FolderPath is not null };
        var statusLabel = new TextBlock { Opacity = 0.7 };

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(pathLabel);
        panel.Children.Add(chooseButton);
        panel.Children.Add(encryptedToggle);
        panel.Children.Add(passphraseBox);
        panel.Children.Add(syncButton);
        panel.Children.Add(statusLabel);

        encryptedToggle.Toggled += (_, _) =>
        {
            _syncConfig.Encrypted = encryptedToggle.IsOn;
            _syncConfig.Save(SyncConfig.DefaultPath);
            passphraseBox.Visibility = encryptedToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
        };

        var dialog = new ContentDialog
        {
            Title = "Synk",
            Content = panel,
            CloseButtonText = "Klar",
            XamlRoot = Content.XamlRoot,
        };

        chooseButton.Click += async (_, _) =>
        {
            var picker = new FolderPicker { SuggestedStartLocation = PickerLocationId.Desktop };
            picker.FileTypeFilter.Add("*");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));

            var folder = await picker.PickSingleFolderAsync();
            if (folder is null) return;

            _syncConfig.FolderPath = folder.Path;
            _syncConfig.Save(SyncConfig.DefaultPath);
            pathLabel.Text = folder.Path;
            syncButton.IsEnabled = true;
        };

        syncButton.Click += async (_, _) =>
        {
            if (_syncConfig.FolderPath is null) return;
            ISyncProvider provider;
            if (_syncConfig.Encrypted)
            {
                if (string.IsNullOrEmpty(passphraseBox.Password))
                {
                    statusLabel.Text = "Ange en lösenfras först";
                    return;
                }
                provider = new EncryptedFolderSyncProvider(
                    Path.Combine(_syncConfig.FolderPath, "hosts.enc"), passphraseBox.Password);
            }
            else
            {
                provider = new FolderSyncProvider(Path.Combine(_syncConfig.FolderPath, "hosts.json"));
            }

            // Filens I/O (kan vara en molnsynkad mapp — Dropbox/Drive/OneDrive
            // — som stallar) och, för den krypterade varianten, PBKDF2-
            // nyckelhärledningen är för tunga för att köras rakt av på
            // UI-tråden — samma fix som redan gjorts i LinuxApp/src/main.rs.
            statusLabel.Text = "Synkar…";
            syncButton.IsEnabled = false;
            try
            {
                await Task.Run(() => _store.Sync(provider));
                statusLabel.Text = "Synkad";
                Refresh();
            }
            catch (Exception ex)
            {
                statusLabel.Text = $"Fel: {ex.Message}";
            }
            finally
            {
                syncButton.IsEnabled = true;
            }
        };

        await dialog.ShowAsync();
    }

    private async void OnHostItemClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is not HostRow row) return;
        var host = FindHost(row.Id);
        if (host is null) return;

        string? password = null;
        if (host.Auth is HostAuth.AskPassword)
        {
            password = await PromptPasswordAsync(host);
            if (password is null) return; // avbrutet
        }

        await OpenDashboardTabAsync(host, password);
    }

    /// <summary>
    /// Värdradens "Mer"-knapp — bygger menyn dynamiskt utifrån aktuella
    /// Funktioner-togglar (motsvarar LinuxApps <c>gio_menu_for</c>, som
    /// utesluter hela menyposten när en toggle är av, inte bara döljer/
    /// inaktiverar en statisk post).
    /// </summary>
    private void OnHostMoreButtonClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: HostRow row } button) return;
        var toggles = _settingsStore.Current();

        var menu = new MenuFlyout();
        var terminalItem = new MenuFlyoutItem { Text = "Terminal" };
        terminalItem.Click += (_, _) => _ = OpenHostFeatureTabAsync(row, (h, p) => OpenTerminalTabAsync(h, p));
        menu.Items.Add(terminalItem);
        if (toggles.ShowDocker)
        {
            var item = new MenuFlyoutItem { Text = "Docker" };
            item.Click += (_, _) => _ = OpenHostFeatureTabAsync(row, OpenDockerTabAsync);
            menu.Items.Add(item);
        }
        if (toggles.ShowCommandLibrary || toggles.ShowSnippets)
        {
            var item = new MenuFlyoutItem { Text = "Kommandon" };
            item.Click += (_, _) => _ = OpenHostFeatureTabAsync(row, OpenCommandsTabAsync);
            menu.Items.Add(item);
        }
        if (toggles.ShowSftpBrowser)
        {
            var item = new MenuFlyoutItem { Text = "Filer" };
            item.Click += (_, _) => _ = OpenHostFeatureTabAsync(row, OpenFilesTabAsync);
            menu.Items.Add(item);
        }

        button.Flyout = menu;
        menu.ShowAt(button);
    }

    /// <summary>Slår upp värden för raden, frågar om lösenord om värden kräver det, öppnar sedan den valda fliken.</summary>
    private async Task OpenHostFeatureTabAsync(HostRow row, Func<Host, string?, Task> openTab)
    {
        var host = FindHost(row.Id);
        if (host is null) return;

        string? password = null;
        if (host.Auth is HostAuth.AskPassword)
        {
            password = await PromptPasswordAsync(host);
            if (password is null) return;
        }

        await openTab(host, password);
    }

    /// <summary>Funktioner-knappen i värdlistans verktygsfält (motsvarar LinuxApps Funktioner-inställningsdialog).</summary>
    private async void OnSettingsClicked(object sender, RoutedEventArgs e)
    {
        var current = _settingsStore.Current();
        var dockerToggle = new ToggleSwitch { Header = "Docker", IsOn = current.ShowDocker };
        var commandsToggle = new ToggleSwitch { Header = "Kommandobibliotek + Snippets", IsOn = current.ShowCommandLibrary };
        var sftpToggle = new ToggleSwitch { Header = "Filer (SFTP)", IsOn = current.ShowSftpBrowser };

        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(dockerToggle);
        panel.Children.Add(commandsToggle);
        panel.Children.Add(sftpToggle);

        var dialog = new ContentDialog
        {
            Title = "Funktioner",
            Content = panel,
            CloseButtonText = "Klar",
            XamlRoot = Content.XamlRoot,
        };

        void Save()
        {
            _settingsStore.Update(current with
            {
                ShowDocker = dockerToggle.IsOn,
                ShowCommandLibrary = commandsToggle.IsOn,
                ShowSnippets = commandsToggle.IsOn,
                ShowSftpBrowser = sftpToggle.IsOn,
            });
        }
        dockerToggle.Toggled += (_, _) => Save();
        commandsToggle.Toggled += (_, _) => Save();
        sftpToggle.Toggled += (_, _) => Save();

        await dialog.ShowAsync();
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

    // MARK: - Terminalflikar

    private async Task OpenTerminalTabAsync(Host host, string? password, string? titleOverride = null)
    {
        var webView = new WebView2();
        var tab = new TabViewItem
        {
            Header = titleOverride ?? host.Alias,
            IconSource = new FontIconSource { FontFamily = new FontFamily("Segoe MDL2 Assets"), Glyph = "" },
            Content = webView,
        };
        SessionTabView.TabItems.Add(tab);
        SessionTabView.SelectedItem = tab;
        UpdateSessionAreaVisibility();

        // Både EnsureCoreWebView2Async (WebView2-runtimen kan saknas) och
        // navigeringen (a.IsSuccess lästes tidigare aldrig, så ett faktiskt
        // navigeringsfel "lyckades" tyst) låg tidigare oskyddade — ett fel
        // här kunde krascha appen istället för att visa ett tydligt
        // felmeddelande i fliken.
        var htmlPath = Path.Combine(AppContext.BaseDirectory, "Assets", "xterm", "terminal.html");
        var navigated = new TaskCompletionSource<bool>();
        void OnNavigationCompleted(WebView2 s, Microsoft.Web.WebView2.Core.CoreWebView2NavigationCompletedEventArgs a) => navigated.TrySetResult(a.IsSuccess);
        try
        {
            await webView.EnsureCoreWebView2Async();
            webView.NavigationCompleted += OnNavigationCompleted;
            webView.CoreWebView2.Navigate(new Uri(htmlPath).AbsoluteUri);
            if (!await navigated.Task)
            {
                throw new InvalidOperationException("terminalsidan gick inte att ladda");
            }
        }
        catch (Exception ex)
        {
            ShowTabError(tab, $"Kunde inte initiera terminalvyn: {ex.Message}");
            return;
        }
        finally
        {
            webView.NavigationCompleted -= OnNavigationCompleted;
        }

        SshSession session;
        try
        {
            session = await Task.Run(() => SshSession.Connect(host, password, _knownHosts));
        }
        catch (SshHostKeyChangedException ex)
        {
            ShowTabError(tab, ex.Message);
            return;
        }
        catch (Exception ex)
        {
            ShowTabError(tab, $"Anslutning misslyckades: {ex.Message}");
            return;
        }

        tab.Tag = session;

        webView.CoreWebView2.WebMessageReceived += (_, args) =>
        {
            var text = args.TryGetWebMessageAsString();
            session.Shell.Write(text);
        };

        session.Shell.DataReceived += (_, args) => FeedTerminal(webView, args);
        session.Shell.Closed += (_, _) => _dispatcher.TryEnqueue(() => OnSessionClosed(tab, session));
    }

    private void ShowTabError(TabViewItem tab, string message)
    {
        tab.Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap, Margin = new Thickness(16), Opacity = 0.7 };
    }

    private void FeedTerminal(WebView2 webView, ShellDataEventArgs args)
    {
        var base64 = Convert.ToBase64String(args.Data);
        _dispatcher.TryEnqueue(() => _ = webView.CoreWebView2.ExecuteScriptAsync($"window.feed('{base64}')"));
    }

    /// <summary>
    /// Fjärrskalet stängde SIG SJÄLVT (exit/Ctrl+D, inte användaren som
    /// stängde fliken) — måste disponera sessionen HÄR, den disponeras
    /// ALDRIG av avsändaren annars (ShellStream.Closed-eventet i sig gör
    /// ingen städning, bara signalerar). `ReferenceEquals`-kollen mot
    /// `tab.Tag` skyddar mot en race med <see cref="OnTabCloseRequested"/>:
    /// vilken av de två som hinner först nollställer `tab.Tag`, så den
    /// andra ser att jobbet redan är taget istället för att dubbeldisponera
    /// samma <see cref="SshSession"/> och skriva över en redan borttagen
    /// flikes innehåll.
    /// </summary>
    private void OnSessionClosed(TabViewItem tab, SshSession session)
    {
        if (!ReferenceEquals(tab.Tag, session)) return;
        tab.Tag = null;
        session.Dispose();
        tab.Content = new TextBlock { Text = "Sessionen avslutades.", Margin = new Thickness(16), Opacity = 0.7 };
    }

    private void OnTabCloseRequested(TabView sender, TabViewTabCloseRequestedEventArgs args)
    {
        if (args.Tab.Tag is IDisposable disposable)
        {
            disposable.Dispose();
            args.Tab.Tag = null;
        }
        sender.TabItems.Remove(args.Tab);
        UpdateSessionAreaVisibility();
    }

    private void UpdateSessionAreaVisibility()
    {
        var hasTabs = SessionTabView.TabItems.Count > 0;
        SessionTabView.Visibility = hasTabs ? Visibility.Visible : Visibility.Collapsed;
        PlaceholderPanel.Visibility = hasTabs ? Visibility.Collapsed : Visibility.Visible;
    }

    // MARK: - Översikt (port av App/DashboardView.swift, samma probe som LinuxApps dashboard.rs)

    /// <summary>
    /// Öppnar värdens översikt: ETT SSH-kommando ger last, minne, disk, temperatur,
    /// drifttid, OS, kärna, IP-adresser, inloggade användare, authorized_keys och
    /// Docker — ingen agent på värden. Samma vy som prototypen i docs/prototyp/
    /// ritar, och samma sak iOS/macOS visar när man trycker på en värd.
    /// </summary>
    private async Task OpenDashboardTabAsync(Host host, string? password)
    {
        var body = new StackPanel { Spacing = 12, Padding = new Thickness(16, 12, 16, 16) };
        var scrolled = new ScrollViewer { Content = body, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };

        var refreshButton = new Button { Content = "\uE72C", FontFamily = new FontFamily("Segoe MDL2 Assets") };
        ToolTipService.SetToolTip(refreshButton, "Uppdatera");
        var terminalButton = new Button { Content = "Terminal" };

        var toolbar = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            Padding = new Thickness(12, 8, 12, 8),
        };
        toolbar.Children.Add(new TextBlock
        {
            Text = $"{host.Alias} — {host.User}@{host.HostName}:{host.Port}",
            VerticalAlignment = VerticalAlignment.Center,
            FontWeight = FontWeights.SemiBold,
        });
        toolbar.Children.Add(refreshButton);
        toolbar.Children.Add(terminalButton);

        var content = new Grid();
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(toolbar, 0);
        Grid.SetRow(scrolled, 1);
        content.Children.Add(toolbar);
        content.Children.Add(scrolled);

        var tab = new TabViewItem
        {
            Header = $"Översikt: {host.Alias}",
            IconSource = new FontIconSource { FontFamily = new FontFamily("Segoe MDL2 Assets"), Glyph = "\uE9D9" },
            Content = content,
        };
        SessionTabView.TabItems.Add(tab);
        SessionTabView.SelectedItem = tab;
        UpdateSessionAreaVisibility();

        refreshButton.Click += (_, _) => _ = RefreshDashboardAsync(host, password, body);
        terminalButton.Click += (_, _) => _ = OpenTerminalTabAsync(host, password);
        await RefreshDashboardAsync(host, password, body);
    }

    private async Task RefreshDashboardAsync(Host host, string? password, StackPanel body)
    {
        body.Children.Clear();
        body.Children.Add(StatusText("Läser av värden…"));

        SystemSnapshot snapshot;
        try
        {
            snapshot = await Task.Run(() => SystemProbe.Snapshot(host, password, _knownHosts));
        }
        catch (Exception ex)
        {
            body.Children.Clear();
            body.Children.Add(StatusText($"Fel: {ex.Message}"));
            return;
        }

        body.Children.Clear();
        body.Children.Add(BuildMetricTiles(snapshot));
        body.Children.Add(SectionHeader("System"));
        body.Children.Add(BuildFactList(host, snapshot));
        body.Children.Add(SectionHeader("Docker"));
        body.Children.Add(BuildContainerSummary(snapshot));
    }

    private static TextBlock SectionHeader(string text) => new()
    {
        Text = text,
        FontWeight = FontWeights.SemiBold,
        FontSize = 12,
        Opacity = 0.7,
        Margin = new Thickness(0, 8, 0, 0),
    };

    /// <summary>Belastning, minne, disk och temperatur som mätarkort, två per rad.</summary>
    private static FrameworkElement BuildMetricTiles(SystemSnapshot snapshot)
    {
        var tiles = new List<FrameworkElement>();

        if (snapshot.Load is { } load)
        {
            var cores = snapshot.CpuCount ?? 0;
            var percent = cores > 0 ? DashboardFormat.Percent(load.One / cores) : 0;
            tiles.Add(MetricTile("Belastning", DashboardFormat.Load(load),
                cores > 0 ? $"{cores} kärnor" : "okänt antal kärnor",
                cores > 0 ? percent : null));
        }

        if (snapshot.Memory is { } memory)
        {
            var percent = DashboardFormat.Percent(memory.UsedFraction);
            tiles.Add(MetricTile("Minne", $"{percent} %",
                $"{DashboardFormat.Bytes(memory.UsedBytes)} av {DashboardFormat.Bytes(memory.TotalBytes)}", percent));
        }

        if ((snapshot.RootDisk ?? snapshot.Disks.FirstOrDefault()) is { } disk)
        {
            tiles.Add(MetricTile($"Disk {disk.Mount}", $"{disk.CapacityPercent} %",
                $"{DashboardFormat.Bytes(disk.UsedBytes)} av {DashboardFormat.Bytes(disk.SizeBytes)}",
                disk.CapacityPercent));
        }

        if (snapshot.Temperatures.Count > 0)
        {
            var warmest = snapshot.Temperatures.MaxBy(t => t.Celsius)!;
            tiles.Add(MetricTile("Temperatur", $"{warmest.Celsius:0} °C", warmest.Label, null));
        }

        if (tiles.Count == 0)
        {
            return StatusText("Värden svarade utan mätvärden — /proc kan vara otillgängligt.");
        }

        var grid = new Grid { ColumnSpacing = 12, RowSpacing = 12 };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (var i = 0; i < tiles.Count; i++)
        {
            if (i % 2 == 0) grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
            Grid.SetRow(tiles[i], i / 2);
            Grid.SetColumn(tiles[i], i % 2);
            grid.Children.Add(tiles[i]);
        }
        return grid;
    }

    /// <summary>Ett kort: etikett, stort värde, mätare färgad efter allvarlighetsgrad, fottext.</summary>
    private static FrameworkElement MetricTile(string label, string value, string foot, int? percent)
    {
        var panel = new StackPanel { Spacing = 6 };
        panel.Children.Add(new TextBlock { Text = label.ToUpperInvariant(), FontSize = 11, Opacity = 0.6, FontWeight = FontWeights.SemiBold });
        panel.Children.Add(new TextBlock { Text = value, FontSize = 22, FontWeight = FontWeights.SemiBold });
        if (percent is { } p)
        {
            panel.Children.Add(new ProgressBar
            {
                Value = p,
                Maximum = 100,
                Foreground = LevelBrush(DashboardFormat.Level(p)),
                HorizontalAlignment = HorizontalAlignment.Stretch,
            });
        }
        panel.Children.Add(new TextBlock { Text = foot, FontSize = 12, Opacity = 0.7 });

        return new Border
        {
            Child = panel,
            Padding = new Thickness(12),
            CornerRadius = new CornerRadius(8),
            BorderThickness = new Thickness(1),
            BorderBrush = ThemeBrush("CardStrokeColorDefaultBrush"),
            Background = ThemeBrush("CardBackgroundFillColorDefaultBrush"),
        };
    }

    /// <summary>Systemets textfakta — resten av det VISION räknar upp för dashboarden.</summary>
    private static FrameworkElement BuildFactList(Host host, SystemSnapshot snapshot)
    {
        var panel = new StackPanel { Spacing = 4 };
        void Fact(string key, string? value)
        {
            if (string.IsNullOrWhiteSpace(value)) return;
            var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            row.Children.Add(new TextBlock { Text = key, Opacity = 0.6, Width = 150 });
            row.Children.Add(new TextBlock { Text = value, TextWrapping = TextWrapping.Wrap });
            panel.Children.Add(row);
        }

        Fact("Värdnamn", snapshot.Hostname ?? host.HostName);
        Fact("Operativsystem", snapshot.Os);
        Fact("Kärna", snapshot.Kernel);
        Fact("Processorer", snapshot.CpuCount?.ToString());
        Fact("Drifttid", snapshot.UptimeSeconds is { } up ? DashboardFormat.Uptime(up) : null);
        Fact("IP-adresser", string.Join(", ", snapshot.Addresses.Select(a => $"{a.Address} ({a.Interface})")));
        Fact("Inloggade", string.Join(", ", snapshot.ActiveUsers.Select(u => u.From is null ? $"{u.User} @ {u.Tty}" : $"{u.User} från {u.From}")));
        Fact("SSH-nycklar", string.Join(", ", snapshot.AuthorizedKeys.Select(k =>
            k.Comment.Length > 0 ? $"{k.Algorithm} {k.Comment}" : k.Algorithm)));

        if (panel.Children.Count == 0) return StatusText("Inga systemuppgifter kunde läsas.");
        return panel;
    }

    private static FrameworkElement BuildContainerSummary(SystemSnapshot snapshot)
    {
        if (snapshot.Containers.Count == 0)
        {
            return StatusText("Ingen container körs (eller docker saknas på värden).");
        }

        var panel = new StackPanel { Spacing = 2 };
        panel.Children.Add(new TextBlock
        {
            Text = $"{snapshot.Containers.Count(c => c.IsRunning)} av {snapshot.Containers.Count} kör",
            Opacity = 0.7,
            FontSize = 12,
        });
        foreach (var container in snapshot.Containers)
        {
            panel.Children.Add(new TextBlock
            {
                Text = $"{container.Name} — {container.Image} ({container.Status})",
                TextWrapping = TextWrapping.Wrap,
            });
        }
        return panel;
    }

    private static Brush LevelBrush(MetricLevel level) => ThemeBrush(level switch
    {
        MetricLevel.Critical => "SystemFillColorCriticalBrush",
        MetricLevel.Warning => "SystemFillColorCautionBrush",
        _ => "SystemFillColorSuccessBrush",
    });

    /// <summary>Systemets temafärger, med en neutral reserv — en saknad nyckel ska
    /// inte ta ner översikten.</summary>
    private static Brush ThemeBrush(string key) =>
        Application.Current.Resources.TryGetValue(key, out var value) && value is Brush brush
            ? brush
            : new SolidColorBrush(Microsoft.UI.Colors.Gray);

    // MARK: - Docker-flik (port av LinuxApps open_docker_view/refresh_docker_list)

    private async Task OpenDockerTabAsync(Host host, string? password)
    {
        var containerList = new StackPanel { Spacing = 0 };
        var scrolled = new ScrollViewer { Content = containerList, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        var refreshButton = new Button { Content = "", FontFamily = new FontFamily("Segoe MDL2 Assets") };
        ToolTipService.SetToolTip(refreshButton, "Uppdatera");

        var toolbar = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            Padding = new Thickness(12, 8, 12, 8),
        };
        toolbar.Children.Add(new TextBlock
        {
            Text = $"Docker: {host.Alias}",
            VerticalAlignment = VerticalAlignment.Center,
            FontWeight = FontWeights.SemiBold,
        });
        toolbar.Children.Add(refreshButton);

        var content = new Grid();
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(toolbar, 0);
        Grid.SetRow(scrolled, 1);
        content.Children.Add(toolbar);
        content.Children.Add(scrolled);

        var tab = new TabViewItem
        {
            Header = $"Docker: {host.Alias}",
            IconSource = new FontIconSource { FontFamily = new FontFamily("Segoe MDL2 Assets"), Glyph = "" },
            Content = content,
        };
        SessionTabView.TabItems.Add(tab);
        SessionTabView.SelectedItem = tab;
        UpdateSessionAreaVisibility();

        refreshButton.Click += (_, _) => _ = RefreshDockerListAsync(host, password, containerList);
        await RefreshDockerListAsync(host, password, containerList);
    }

    private async Task RefreshDockerListAsync(Host host, string? password, StackPanel list)
    {
        list.Children.Clear();
        list.Children.Add(StatusText("Laddar…"));

        IReadOnlyList<DockerContainer> containers;
        try
        {
            containers = await Task.Run(() => DockerService.List(host, password, _knownHosts));
        }
        catch (Exception ex)
        {
            list.Children.Clear();
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
            return;
        }

        list.Children.Clear();
        if (containers.Count == 0)
        {
            list.Children.Add(StatusText("Inga containrar hittades."));
            return;
        }

        foreach (var container in containers)
        {
            list.Children.Add(BuildContainerRow(host, password, list, container));
        }
    }

    private static TextBlock StatusText(string text) => new() { Text = text, Opacity = 0.7, Margin = new Thickness(12) };

    private FrameworkElement BuildContainerRow(Host host, string? password, StackPanel list, DockerContainer container)
    {
        var textPanel = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        textPanel.Children.Add(new TextBlock { Text = container.Name, FontWeight = FontWeights.SemiBold });
        textPanel.Children.Add(new TextBlock { Text = $"{container.Image} — {container.Status}", Opacity = 0.7, FontSize = 12 });

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, VerticalAlignment = VerticalAlignment.Center };

        async void RunAndRefresh(Action action)
        {
            try
            {
                await Task.Run(action);
            }
            catch
            {
                // felet syns naturligt genom att statusen inte ändras vid nästa uppdatering
            }
            await RefreshDockerListAsync(host, password, list);
        }

        if (container.IsRunning)
        {
            var stopBtn = IconButton("", "Stoppa");
            stopBtn.Click += (_, _) => RunAndRefresh(() => DockerService.Stop(host, password, _knownHosts, container.Id));
            var restartBtn = IconButton("", "Starta om");
            restartBtn.Click += (_, _) => RunAndRefresh(() => DockerService.Restart(host, password, _knownHosts, container.Id));
            var shellBtn = IconButton("", "Shell");
            shellBtn.Click += (_, _) =>
            {
                var shellHost = CloneHostForShell(host, container);
                _ = OpenTerminalTabAsync(shellHost, password, $"{host.Alias}: {container.Name}");
            };
            buttons.Children.Add(stopBtn);
            buttons.Children.Add(restartBtn);
            buttons.Children.Add(shellBtn);
        }
        else
        {
            var startBtn = IconButton("", "Starta");
            startBtn.Click += (_, _) => RunAndRefresh(() => DockerService.Start(host, password, _knownHosts, container.Id));
            buttons.Children.Add(startBtn);
        }

        var logsBtn = IconButton("", "Loggar");
        logsBtn.Click += (_, _) => _ = ShowDockerLogsAsync(host, password, container);
        buttons.Children.Add(logsBtn);

        var row = new Grid { Padding = new Thickness(8, 6, 8, 6) };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(textPanel, 0);
        Grid.SetColumn(buttons, 1);
        row.Children.Add(textPanel);
        row.Children.Add(buttons);

        return new Border
        {
            Child = row,
            BorderBrush = Application.Current.Resources["CardStrokeColorDefaultBrush"] as Brush,
            BorderThickness = new Thickness(0, 0, 0, 1),
        };
    }

    private static Button IconButton(string glyph, string toolTip)
    {
        var button = new Button { Content = glyph, FontFamily = new FontFamily("Segoe MDL2 Assets") };
        ToolTipService.SetToolTip(button, toolTip);
        return button;
    }

    /// <summary>
    /// Egen kopia av värden med <c>docker exec</c> som startkommando — samma
    /// mönster som LinuxApps <c>shell_host.startup_command</c>/<c>shell_host.alias</c>;
    /// muterar ALDRIG den delade <see cref="Host"/>-instansen från <see cref="_store"/>.
    /// </summary>
    private static Host CloneHostForShell(Host host, DockerContainer container) =>
        CloneHostWithStartupCommand(host, $"{host.Alias}: {container.Name}", DockerService.ExecShellCommand(container.Id));

    private async Task ShowDockerLogsAsync(Host host, string? password, DockerContainer container)
    {
        string logs;
        try
        {
            logs = await Task.Run(() => DockerService.Logs(host, password, _knownHosts, container.Id));
        }
        catch (Exception ex)
        {
            logs = $"Fel: {ex.Message}";
        }

        var textBlock = new TextBlock
        {
            Text = logs,
            FontFamily = new FontFamily("Consolas"),
            IsTextSelectionEnabled = true,
        };
        var scroll = new ScrollViewer
        {
            Content = textBlock,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            Width = 700,
            Height = 500,
        };
        var dialog = new ContentDialog
        {
            Title = $"Loggar: {container.Name}",
            Content = scroll,
            CloseButtonText = "Stäng",
            XamlRoot = Content.XamlRoot,
        };
        await dialog.ShowAsync();
    }

    // MARK: - Kommandon-flik (port av LinuxApps open_command_library_view/refresh_command_library_list)

    private async Task OpenCommandsTabAsync(Host host, string? password)
    {
        var list = new StackPanel { Spacing = 0 };
        var scrolled = new ScrollViewer { Content = list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        var addButton = IconButton("", "Ny snippet");

        var toolbar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, Padding = new Thickness(12, 8, 12, 8) };
        toolbar.Children.Add(new TextBlock
        {
            Text = $"Kommandon: {host.Alias}",
            VerticalAlignment = VerticalAlignment.Center,
            FontWeight = FontWeights.SemiBold,
        });
        toolbar.Children.Add(addButton);

        var content = new Grid();
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(toolbar, 0);
        Grid.SetRow(scrolled, 1);
        content.Children.Add(toolbar);
        content.Children.Add(scrolled);

        var tab = new TabViewItem
        {
            Header = $"Kommandon: {host.Alias}",
            IconSource = new FontIconSource { FontFamily = new FontFamily("Segoe MDL2 Assets"), Glyph = "" },
            Content = content,
        };
        SessionTabView.TabItems.Add(tab);
        SessionTabView.SelectedItem = tab;
        UpdateSessionAreaVisibility();

        addButton.Click += (_, _) => _ = ShowSnippetEditDialogAsync(host, password, list, existing: null);

        RefreshCommandsList(host, password, list);
    }

    private void RefreshCommandsList(Host host, string? password, StackPanel list)
    {
        list.Children.Clear();
        foreach (var snippet in _snippetStore.All())
        {
            list.Children.Add(BuildSnippetRow(host, password, list, snippet));
        }
        foreach (var entry in CommandLibrary.All)
        {
            list.Children.Add(BuildLibraryEntryRow(host, password, entry));
        }
    }

    private FrameworkElement BuildSnippetRow(Host host, string? password, StackPanel list, Snippet snippet)
    {
        var textPanel = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        textPanel.Children.Add(new TextBlock { Text = snippet.Name, FontWeight = FontWeights.SemiBold });
        textPanel.Children.Add(new TextBlock { Text = snippet.Template, Opacity = 0.7, FontSize = 12 });

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, VerticalAlignment = VerticalAlignment.Center };

        var runBtn = IconButton("", "Kör");
        runBtn.Click += (_, _) => _ = RunSnippetAsync(host, password, snippet);
        var editBtn = IconButton("", "Redigera");
        editBtn.Click += (_, _) => _ = ShowSnippetEditDialogAsync(host, password, list, existing: snippet);
        var deleteBtn = IconButton("", "Ta bort");
        deleteBtn.Click += (_, _) =>
        {
            _snippetStore.Delete(snippet.Id);
            RefreshCommandsList(host, password, list);
        };
        buttons.Children.Add(runBtn);
        buttons.Children.Add(editBtn);
        buttons.Children.Add(deleteBtn);

        return CommandRow(textPanel, buttons);
    }

    private FrameworkElement BuildLibraryEntryRow(Host host, string? password, CommandLibraryEntry entry)
    {
        var subtitle = $"[{entry.Category}] {entry.Summary}" + (entry.Example is { } ex ? $" — t.ex. {ex}" : "");
        var textPanel = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        textPanel.Children.Add(new TextBlock { Text = entry.Command, FontWeight = FontWeights.SemiBold });
        textPanel.Children.Add(new TextBlock { Text = subtitle, Opacity = 0.7, FontSize = 12, TextWrapping = TextWrapping.Wrap });

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, VerticalAlignment = VerticalAlignment.Center };

        if (entry.DocsUrl is { } docsUrl)
        {
            var docsBtn = IconButton("", "Dokumentation");
            docsBtn.Click += (_, _) => _ = Launcher.LaunchUriAsync(new Uri(docsUrl));
            buttons.Children.Add(docsBtn);
        }

        var runBtn = IconButton("", "Kör");
        runBtn.Click += (_, _) => _ = RunSnippetAsync(host, password, entry.AsSnippet);
        buttons.Children.Add(runBtn);

        return CommandRow(textPanel, buttons);
    }

    private static Border CommandRow(FrameworkElement textPanel, FrameworkElement buttons)
    {
        var row = new Grid { Padding = new Thickness(8, 6, 8, 6) };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(textPanel, 0);
        Grid.SetColumn(buttons, 1);
        row.Children.Add(textPanel);
        row.Children.Add(buttons);

        return new Border
        {
            Child = row,
            BorderBrush = Application.Current.Resources["CardStrokeColorDefaultBrush"] as Brush,
            BorderThickness = new Thickness(0, 0, 0, 1),
        };
    }

    private async Task RunSnippetAsync(Host host, string? password, Snippet snippet)
    {
        var variableNames = snippet.VariableNames();
        var values = variableNames.Count == 0
            ? new Dictionary<string, string>()
            : await PromptSnippetVariablesAsync(snippet, variableNames);
        if (values is null) return; // avbrutet

        var shellHost = CloneHostWithStartupCommand(host, $"{host.Alias}: {snippet.Name}", snippet.Rendered(values));
        await OpenTerminalTabAsync(shellHost, password, shellHost.Alias);
    }

    private async Task<Dictionary<string, string>?> PromptSnippetVariablesAsync(Snippet snippet, IReadOnlyList<string> variableNames)
    {
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock { Text = snippet.Template, Opacity = 0.7, TextWrapping = TextWrapping.Wrap });
        var boxes = variableNames.ToDictionary(name => name, name => new TextBox { PlaceholderText = name });
        foreach (var box in boxes.Values) panel.Children.Add(box);

        var dialog = new ContentDialog
        {
            Title = snippet.Name,
            Content = panel,
            PrimaryButtonText = "Kör",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary) return null;

        return boxes.ToDictionary(kv => kv.Key, kv => kv.Value.Text);
    }

    private async Task ShowSnippetEditDialogAsync(Host host, string? password, StackPanel list, Snippet? existing)
    {
        var nameBox = new TextBox { PlaceholderText = "Namn", Text = existing?.Name ?? "" };
        var templateBox = new TextBox { PlaceholderText = "Kommando (t.ex. docker restart {{service}})", Text = existing?.Template ?? "" };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(nameBox);
        panel.Children.Add(templateBox);

        var dialog = new ContentDialog
        {
            Title = "Snippet",
            Content = panel,
            PrimaryButtonText = existing is null ? "Lägg till" : "Spara",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary) return;

        var name = nameBox.Text.Trim();
        var template = templateBox.Text.Trim();
        if (name.Length == 0 || template.Length == 0) return;

        var snippet = existing ?? Snippet.Create(name, template);
        snippet.Name = name;
        snippet.Template = template;
        _snippetStore.Upsert(snippet);
        RefreshCommandsList(host, password, list);
    }

    /// <summary>
    /// Egen kopia av värden med ett annat startkommando — samma mönster som
    /// <see cref="CloneHostForShell"/>/LinuxApps launch_rendered_command,
    /// muterar ALDRIG den delade Host-instansen från _store.
    /// </summary>
    private static Host CloneHostWithStartupCommand(Host host, string alias, string startupCommand) => new()
    {
        Id = host.Id,
        Alias = alias,
        HostName = host.HostName,
        User = host.User,
        Port = host.Port,
        Tags = host.Tags,
        Auth = host.Auth,
        IsFavorite = host.IsFavorite,
        ColorTag = host.ColorTag,
        Platform = host.Platform,
        StartupCommand = startupCommand,
        JumpHostId = host.JumpHostId,
        MacAddress = host.MacAddress,
        ModifiedAt = host.ModifiedAt,
    };

    // MARK: - Filer-flik (port av App/SFTPBrowserModel.swift / LinuxApps open_sftp_view)

    private sealed class FilesTabState
    {
        public required SftpBrowserSession Sftp { get; init; }
        public required Host Host { get; init; }
        public required string? Password { get; init; }
        public string CurrentPath { get; set; } = ".";
    }

    /// <summary>
    /// Faktisk absolut sökväg via ett engångskommando (motsvarar Swiftsidans
    /// <c>SFTPClient.realpath</c>/Rusts samma anrop) — SSH.NETs SftpClient
    /// saknar en egen realpath-motsvarighet (verifierat mot API:t). SFTP:s
    /// currentPath och exec-kanalens arbetskatalog delar typiskt startkatalog
    /// men är INTE garanterat samma sak, samma kommentar som Swift/Rust-sidan.
    /// </summary>
    private string ResolveRealPath(FilesTabState state, string relativePath) =>
        SshSession.RunCommand(state.Host, state.Password, _knownHosts, $"cd {ArchiveOperations.ShellQuote(relativePath)} && pwd").Trim();

    private async Task OpenFilesTabAsync(Host host, string? password)
    {
        SftpBrowserSession sftp;
        try
        {
            sftp = await Task.Run(() => SftpBrowserSession.Connect(host, password, _knownHosts));
        }
        catch (Exception ex)
        {
            var errorTab = new TabViewItem
            {
                Header = $"Filer: {host.Alias}",
                Content = new TextBlock { Text = $"Anslutning misslyckades: {ex.Message}", Margin = new Thickness(16), TextWrapping = TextWrapping.Wrap },
            };
            SessionTabView.TabItems.Add(errorTab);
            SessionTabView.SelectedItem = errorTab;
            UpdateSessionAreaVisibility();
            return;
        }

        var state = new FilesTabState { Sftp = sftp, Host = host, Password = password };
        var list = new StackPanel { Spacing = 0 };
        var scrolled = new ScrollViewer { Content = list, VerticalScrollBarVisibility = ScrollBarVisibility.Auto };
        var pathLabel = new TextBlock { VerticalAlignment = VerticalAlignment.Center, HorizontalAlignment = HorizontalAlignment.Stretch };
        var upButton = IconButton("", "Upp en nivå");
        var mkdirButton = IconButton("", "Ny mapp");
        var refreshButton = IconButton("", "Uppdatera");

        var toolbar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, Padding = new Thickness(12, 8, 12, 8) };
        toolbar.Children.Add(upButton);
        toolbar.Children.Add(pathLabel);
        toolbar.Children.Add(mkdirButton);
        toolbar.Children.Add(refreshButton);

        var content = new Grid();
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(toolbar, 0);
        Grid.SetRow(scrolled, 1);
        content.Children.Add(toolbar);
        content.Children.Add(scrolled);

        var tab = new TabViewItem
        {
            Header = $"Filer: {host.Alias}",
            IconSource = new FontIconSource { FontFamily = new FontFamily("Segoe MDL2 Assets"), Glyph = "" },
            Content = content,
            Tag = sftp,
        };
        SessionTabView.TabItems.Add(tab);
        SessionTabView.SelectedItem = tab;
        UpdateSessionAreaVisibility();

        upButton.Click += (_, _) =>
        {
            if (state.CurrentPath == ".") return;
            var slash = state.CurrentPath.LastIndexOf('/');
            state.CurrentPath = slash >= 0 ? state.CurrentPath[..slash] : ".";
            _ = RefreshFilesListAsync(state, list, pathLabel);
        };
        mkdirButton.Click += (_, _) => _ = PromptNewFolderAsync(state, list, pathLabel);
        refreshButton.Click += (_, _) => _ = RefreshFilesListAsync(state, list, pathLabel);

        await RefreshFilesListAsync(state, list, pathLabel);
    }

    private static string JoinedPath(string basePath, string name) => basePath == "." ? name : $"{basePath}/{name}";

    private async Task RefreshFilesListAsync(FilesTabState state, StackPanel list, TextBlock pathLabel)
    {
        pathLabel.Text = state.CurrentPath;
        list.Children.Clear();
        list.Children.Add(StatusText("Laddar…"));

        IReadOnlyList<SftpEntry> entries;
        try
        {
            entries = await Task.Run(() => state.Sftp.List(state.CurrentPath));
        }
        catch (Exception ex)
        {
            list.Children.Clear();
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
            return;
        }

        list.Children.Clear();
        foreach (var entry in entries)
        {
            list.Children.Add(BuildFileRow(state, list, pathLabel, entry));
        }
    }

    private FrameworkElement BuildFileRow(FilesTabState state, StackPanel list, TextBlock pathLabel, SftpEntry entry)
    {
        var textPanel = new StackPanel { VerticalAlignment = VerticalAlignment.Center };
        textPanel.Children.Add(new TextBlock { Text = entry.Name, FontWeight = FontWeights.SemiBold });
        textPanel.Children.Add(new TextBlock { Text = entry.IsDirectory ? "Mapp" : $"{entry.Size} bytes", Opacity = 0.7, FontSize = 12 });

        var openButton = new Button { Content = textPanel, Background = null, BorderThickness = new Thickness(0), HorizontalContentAlignment = HorizontalAlignment.Left };
        openButton.Click += (_, _) =>
        {
            var fullPath = JoinedPath(state.CurrentPath, entry.Name);
            if (entry.IsDirectory)
            {
                state.CurrentPath = fullPath;
                _ = RefreshFilesListAsync(state, list, pathLabel);
            }
            else
            {
                _ = OpenFileEditorAsync(state, fullPath);
            }
        };

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4, VerticalAlignment = VerticalAlignment.Center };

        var renameBtn = IconButton("", "Döp om");
        renameBtn.Click += (_, _) => _ = PromptRenameAsync(state, list, pathLabel, entry);
        var deleteBtn = IconButton("", "Ta bort");
        deleteBtn.Click += (_, _) => _ = DeleteEntryAsync(state, list, pathLabel, entry);
        var permissionsBtn = IconButton("", "Rättigheter/ägare");
        permissionsBtn.Click += (_, _) => _ = PromptPermissionsAsync(state, list, pathLabel, entry);
        buttons.Children.Add(permissionsBtn);
        buttons.Children.Add(renameBtn);
        buttons.Children.Add(deleteBtn);

        if (entry.IsDirectory)
        {
            var compressBtn = IconButton("", "Komprimera (tar.gz)");
            compressBtn.Click += (_, _) => _ = CompressEntryAsync(state, list, entry);
            buttons.Children.Add(compressBtn);
        }
        else if (entry.Name.EndsWith(".tar.gz") || entry.Name.EndsWith(".tgz") || entry.Name.EndsWith(".zip"))
        {
            var extractBtn = IconButton("", "Packa upp");
            extractBtn.Click += (_, _) => _ = ExtractEntryAsync(state, list, pathLabel, entry);
            buttons.Children.Add(extractBtn);
        }

        var row = new Grid { Padding = new Thickness(8, 6, 8, 6) };
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(openButton, 0);
        Grid.SetColumn(buttons, 1);
        row.Children.Add(openButton);
        row.Children.Add(buttons);

        return new Border
        {
            Child = row,
            BorderBrush = Application.Current.Resources["CardStrokeColorDefaultBrush"] as Brush,
            BorderThickness = new Thickness(0, 0, 0, 1),
        };
    }

    private async Task OpenFileEditorAsync(FilesTabState state, string path)
    {
        byte[] bytes;
        try
        {
            bytes = await Task.Run(() => state.Sftp.ReadFile(path));
        }
        catch (Exception ex)
        {
            var errorDialog = new ContentDialog
            {
                Title = path,
                Content = $"Fel: {ex.Message}",
                CloseButtonText = "Stäng",
                XamlRoot = Content.XamlRoot,
            };
            await errorDialog.ShowAsync();
            return;
        }

        var isBinary = !SftpBrowserSession.TryDecodeUtf8(bytes, out var text);
        var textBox = new TextBox
        {
            Text = isBinary ? $"(binärt innehåll, {bytes.Length} bytes — kan inte visas eller redigeras som text)" : text,
            AcceptsReturn = true,
            IsReadOnly = isBinary,
            FontFamily = new FontFamily("Consolas"),
            TextWrapping = TextWrapping.NoWrap,
            Height = 400,
        };
        var scroll = new ScrollViewer { Content = textBox, Width = 700 };

        var dialog = new ContentDialog
        {
            Title = path,
            Content = scroll,
            PrimaryButtonText = isBinary ? null : "Spara",
            CloseButtonText = "Stäng",
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary || isBinary) return;

        try
        {
            await Task.Run(() => state.Sftp.WriteFile(path, Encoding.UTF8.GetBytes(textBox.Text)));
        }
        catch (Exception ex)
        {
            var errorDialog = new ContentDialog
            {
                Title = "Kunde inte spara",
                Content = ex.Message,
                CloseButtonText = "Stäng",
                XamlRoot = Content.XamlRoot,
            };
            await errorDialog.ShowAsync();
        }
    }

    private async Task PromptNewFolderAsync(FilesTabState state, StackPanel list, TextBlock pathLabel)
    {
        var nameBox = new TextBox { PlaceholderText = "Mappnamn" };
        var dialog = new ContentDialog
        {
            Title = "Ny mapp",
            Content = nameBox,
            PrimaryButtonText = "Skapa",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary) return;

        var name = nameBox.Text.Trim();
        if (name.Length == 0) return;

        try
        {
            await Task.Run(() => state.Sftp.CreateDirectory(JoinedPath(state.CurrentPath, name)));
        }
        catch (Exception ex)
        {
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
        }
        await RefreshFilesListAsync(state, list, pathLabel);
    }

    /// <summary>
    /// Alla SFTP-åtgärder i den här fliken körs via <see cref="Task.Run"/> —
    /// mkdir/döp om/rättigheter/borttagning gjorde det tidigare INTE (bara
    /// läs/skriv/lista/komprimera/packa-upp gjorde), och blockerade UI-
    /// tråden under hela nätverksrundturen.
    /// </summary>
    private async Task DeleteEntryAsync(FilesTabState state, StackPanel list, TextBlock pathLabel, SftpEntry entry)
    {
        var fullPath = JoinedPath(state.CurrentPath, entry.Name);
        try
        {
            await Task.Run(() =>
            {
                if (entry.IsDirectory) state.Sftp.RemoveDirectory(fullPath);
                else state.Sftp.RemoveFile(fullPath);
            });
        }
        catch (Exception ex)
        {
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
            return;
        }
        await RefreshFilesListAsync(state, list, pathLabel);
    }

    private async Task PromptRenameAsync(FilesTabState state, StackPanel list, TextBlock pathLabel, SftpEntry entry)
    {
        var nameBox = new TextBox { Text = entry.Name };
        var dialog = new ContentDialog
        {
            Title = "Döp om",
            Content = nameBox,
            PrimaryButtonText = "Döp om",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary) return;

        var newName = nameBox.Text.Trim();
        if (newName.Length == 0 || newName == entry.Name) return;

        try
        {
            await Task.Run(() => state.Sftp.Rename(JoinedPath(state.CurrentPath, entry.Name), JoinedPath(state.CurrentPath, newName)));
        }
        catch (Exception ex)
        {
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
        }
        await RefreshFilesListAsync(state, list, pathLabel);
    }

    /// <summary>mode: oktal sträng utan 0-prefix, t.ex. "644"/"755" — samma notation som chmod på kommandoraden. uid/gid: numeriska ID:n, SFTP v3 känner bara till UID/GID, aldrig namn.</summary>
    private async Task PromptPermissionsAsync(FilesTabState state, StackPanel list, TextBlock pathLabel, SftpEntry entry)
    {
        var modeBox = new TextBox { PlaceholderText = "Behörighet (oktalt, t.ex. 644)" };
        var uidBox = new TextBox { PlaceholderText = "UID (t.ex. 1000)" };
        var gidBox = new TextBox { PlaceholderText = "GID (t.ex. 1000)" };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(modeBox);
        panel.Children.Add(uidBox);
        panel.Children.Add(gidBox);

        var dialog = new ContentDialog
        {
            Title = $"Rättigheter/ägare: {entry.Name}",
            Content = panel,
            PrimaryButtonText = "Verkställ",
            CloseButtonText = "Avbryt",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var result = await dialog.ShowAsync();
        if (result != ContentDialogResult.Primary) return;

        var fullPath = JoinedPath(state.CurrentPath, entry.Name);

        // Parsning/validering är billig och synkron — görs FÖRE Task.Run,
        // bara de faktiska SFTP-anropen (nätverksrundturer) behöver köras
        // där.
        short? mode = null;
        if (!string.IsNullOrWhiteSpace(modeBox.Text))
        {
            if (!TryParseOctalMode(modeBox.Text, out var parsedMode))
            {
                list.Children.Add(StatusText("Ogiltig behörighet — ange tre oktala siffror, t.ex. 644."));
                return;
            }
            mode = parsedMode;
        }
        (int Uid, int Gid)? owner = null;
        if (!string.IsNullOrWhiteSpace(uidBox.Text) && !string.IsNullOrWhiteSpace(gidBox.Text))
        {
            if (!int.TryParse(uidBox.Text, out var uid) || !int.TryParse(gidBox.Text, out var gid))
            {
                list.Children.Add(StatusText("Ogiltigt UID/GID — ange numeriska ID:n, t.ex. 1000."));
                return;
            }
            owner = (uid, gid);
        }

        try
        {
            await Task.Run(() =>
            {
                if (mode is { } m) state.Sftp.SetPermissions(fullPath, m);
                if (owner is { } o) state.Sftp.SetOwner(fullPath, o.Uid, o.Gid);
            });
        }
        catch (Exception ex)
        {
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
        }
        await RefreshFilesListAsync(state, list, pathLabel);
    }

    private static bool TryParseOctalMode(string text, out short mode)
    {
        mode = 0;
        if (text.Length != 3 || !text.All(c => c is >= '0' and <= '7')) return false;
        mode = Convert.ToInt16(text, 8);
        return true;
    }

    /// <summary>Komprimerar mappens INNEHÅLL (. inifrån mappen själv), arkivet hamnar bredvid mappen (i state.CurrentPath, inte inuti den) — samma mönster som LinuxApps compress_button.</summary>
    private async Task CompressEntryAsync(FilesTabState state, StackPanel list, SftpEntry entry)
    {
        try
        {
            var fullDir = JoinedPath(state.CurrentPath, entry.Name);
            var absoluteDir = await Task.Run(() => ResolveRealPath(state, fullDir));
            var archiveName = $"../{entry.Name}.tar.gz";
            var command = ArchiveOperations.CreateTarGzCommand(new[] { "." }, archiveName, absoluteDir);
            await Task.Run(() => SshSession.RunCommand(state.Host, state.Password, _knownHosts, command));
        }
        catch (Exception ex)
        {
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
        }
    }

    /// <summary>Packar upp ett arkiv i SAMMA katalog det ligger i. Formatet avgörs av filändelsen.</summary>
    private async Task ExtractEntryAsync(FilesTabState state, StackPanel list, TextBlock pathLabel, SftpEntry entry)
    {
        try
        {
            var absoluteDir = await Task.Run(() => ResolveRealPath(state, state.CurrentPath));
            var command = entry.Name.EndsWith(".zip")
                ? ArchiveOperations.ExtractZipCommand(entry.Name, absoluteDir)
                : ArchiveOperations.ExtractTarGzCommand(entry.Name, absoluteDir);
            await Task.Run(() => SshSession.RunCommand(state.Host, state.Password, _knownHosts, command));
        }
        catch (Exception ex)
        {
            list.Children.Add(StatusText($"Fel: {ex.Message}"));
            return;
        }
        await RefreshFilesListAsync(state, list, pathLabel);
    }
}
