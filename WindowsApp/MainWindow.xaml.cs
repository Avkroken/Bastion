using System.Collections.ObjectModel;
using Bastion.Core;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

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
    private readonly ObservableCollection<HostRow> _rows = new();

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

    private void OnHostItemClicked(object sender, ItemClickEventArgs e)
    {
        if (e.ClickedItem is HostRow row)
        {
            ContentPlaceholder.Text = $"{row.Alias} vald — SSH-anslutning är inte inkopplad än.";
        }
    }
}
