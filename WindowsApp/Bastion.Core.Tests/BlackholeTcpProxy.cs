using System.Net;
using System.Net.Sockets;

namespace Bastion.Core.Tests;

/// <summary>
/// Lokal TCP-proxy som först vidarebefordrar normalt och sedan kan sluta
/// vidarebefordra UTAN att stänga klientens socket. Det reproducerar den
/// viktiga failure mode som en vanlig process.Kill()-test inte kan bevisa:
/// TCP kan fortfarande se etablerad ut lokalt medan all trafik svart-hålas.
/// </summary>
internal sealed class BlackholeTcpProxy : IDisposable
{
    private readonly TcpListener _listener;
    private readonly int _targetPort;
    private readonly CancellationTokenSource _lifetime = new();
    private readonly CancellationTokenSource _forwarding = new();
    private readonly TaskCompletionSource<bool> _ready = new(TaskCreationOptions.RunContinuationsAsynchronously);
    private TcpClient? _clientSide;
    private TcpClient? _serverSide;
    private int _disposed;

    private BlackholeTcpProxy(int targetPort)
    {
        _targetPort = targetPort;
        _listener = new TcpListener(IPAddress.Loopback, 0);
        _listener.Start();
        Port = ((IPEndPoint)_listener.LocalEndpoint).Port;
        _ = AcceptAndForwardAsync();
    }

    public int Port { get; }

    public static BlackholeTcpProxy Start(int targetPort) => new(targetPort);

    public void Blackhole()
    {
        _ready.Task.GetAwaiter().GetResult();
        _forwarding.Cancel();
    }

    private async Task AcceptAndForwardAsync()
    {
        try
        {
            _clientSide = await _listener.AcceptTcpClientAsync(_lifetime.Token);
            _serverSide = new TcpClient();
            await _serverSide.ConnectAsync(IPAddress.Loopback, _targetPort, _lifetime.Token);
            _ready.TrySetResult(true);

            await Task.WhenAll(
                ForwardAsync(_clientSide.GetStream(), _serverSide.GetStream(), _forwarding.Token),
                ForwardAsync(_serverSide.GetStream(), _clientSide.GetStream(), _forwarding.Token));
        }
        catch (OperationCanceledException) when (_lifetime.IsCancellationRequested || _forwarding.IsCancellationRequested)
        {
            _ready.TrySetCanceled();
        }
        catch (ObjectDisposedException ex) when (!_ready.Task.IsCompleted)
        {
            _ready.TrySetException(ex);
        }
        catch (SocketException ex) when (!_ready.Task.IsCompleted)
        {
            _ready.TrySetException(ex);
        }
        catch (IOException ex) when (!_ready.Task.IsCompleted)
        {
            _ready.TrySetException(ex);
        }
        catch (ObjectDisposedException) when (_lifetime.IsCancellationRequested || _forwarding.IsCancellationRequested)
        {
            // Normal städning av en pågående ReadAsync/WriteAsync.
        }
        catch (IOException) when (_lifetime.IsCancellationRequested || _forwarding.IsCancellationRequested)
        {
            // Samma städningsfall på NetworkStream-nivå.
        }
    }

    private static async Task ForwardAsync(NetworkStream input, NetworkStream output, CancellationToken cancellationToken)
    {
        var buffer = new byte[16 * 1024];
        while (!cancellationToken.IsCancellationRequested)
        {
            var read = await input.ReadAsync(buffer, cancellationToken);
            if (read == 0) return;
            await output.WriteAsync(buffer.AsMemory(0, read), cancellationToken);
        }
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
        _forwarding.Cancel();
        _lifetime.Cancel();
        _listener.Stop();
        _clientSide?.Dispose();
        _serverSide?.Dispose();
        _forwarding.Dispose();
        _lifetime.Dispose();
    }
}
