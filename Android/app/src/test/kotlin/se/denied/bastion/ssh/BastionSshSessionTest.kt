package se.denied.bastion.ssh

import org.apache.sshd.client.SshClient
import org.apache.sshd.core.CoreModuleProperties
import org.apache.sshd.server.SshServer
import org.apache.sshd.server.auth.password.PasswordAuthenticator
import org.apache.sshd.server.command.CommandFactory
import org.apache.sshd.server.keyprovider.SimpleGeneratorHostKeyProvider
import org.junit.After
import org.junit.Before
import org.junit.Test
import java.io.OutputStream
import java.nio.file.Files
import java.nio.file.Path
import java.time.Duration
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * Riktig anslutning mot en riktig SSH-server, inte en mockad kanal — samma
 * princip som SSHCore-testerna på Swift-sidan (LoopbackServer-mönstret).
 * Servern körs in-process via MINA SSHD:s egen serverimplementation, ingen
 * beroende av systemets sshd eller nätverksåtkomst utanför localhost.
 */
class BastionSshSessionTest {

    private lateinit var server: SshServer
    private lateinit var knownHostsFile: Path
    private var port: Int = 0

    @Before
    fun startServer() {
        knownHostsFile = Files.createTempDirectory("bastion-known-hosts-test").resolve("known_hosts")
        server = createServer(0)
        port = server.port
    }

    @After
    fun stopServer() {
        server.stop(true)
    }

    @Test
    fun `connect authenticate run and get real output back`() {
        BastionSshSession(
            host = "127.0.0.1",
            port = port,
            user = "tester",
            knownHostsFile = knownHostsFile,
        ).use { session ->
            session.connect(password = "s3cret")
            val output = session.run("echo hello-from-bastion")
            assertEquals("hello-from-bastion\n", output)
        }
    }

    @Test
    fun `first seen host key is persisted same key reconnects and changed key is rejected`() {
        BastionSshSession(
            host = "127.0.0.1",
            port = port,
            user = "tester",
            knownHostsFile = knownHostsFile,
        ).use { session ->
            session.connect(password = "s3cret")
        }

        assertTrue(Files.exists(knownHostsFile))
        assertTrue(String(Files.readAllBytes(knownHostsFile), Charsets.UTF_8).contains("127.0.0.1"))

        BastionSshSession(
            host = "127.0.0.1",
            port = port,
            user = "tester",
            knownHostsFile = knownHostsFile,
        ).use { session ->
            session.connect(password = "s3cret")
        }

        server.stop(true)
        server = createServer(port)

        val changedKeyResult = runCatching {
            BastionSshSession(
                host = "127.0.0.1",
                port = port,
                user = "tester",
                knownHostsFile = knownHostsFile,
            ).use { session ->
                session.connect(password = "s3cret", timeoutSeconds = 5)
            }
        }
        assertTrue(changedKeyResult.isFailure, "ändrad host key måste avvisas")
    }

    @Test
    fun `unusable known hosts path rejects connection`() {
        val directoryInsteadOfFile = Files.createTempDirectory("bastion-unusable-known-hosts")

        val result = runCatching {
            BastionSshSession(
                host = "127.0.0.1",
                port = port,
                user = "tester",
                knownHostsFile = directoryInsteadOfFile,
            ).use { session ->
                session.connect(password = "s3cret", timeoutSeconds = 5)
            }
        }

        assertTrue(result.isFailure, "known_hosts-fel måste avvisa anslutningen")
    }

    @Test
    fun `response-bearing heartbeat keeps an idle authenticated session usable`() {
        BastionSshSession(
            host = "127.0.0.1",
            port = port,
            user = "tester",
            knownHostsFile = knownHostsFile,
            heartbeatIntervalSeconds = 1,
            heartbeatMaxNoReply = 2,
        ).use { session ->
            session.connect(password = "s3cret")
            Thread.sleep(1_200)
            assertEquals("still-alive\n", session.run("echo still-alive"))
        }
    }

    @Test
    fun `heartbeat configuration requires replies and has a finite failure threshold`() {
        val client = SshClient.setUpDefaultClient()
        BastionSshSession.configureHeartbeat(client, intervalSeconds = 1, maxNoReply = 2)

        assertEquals(Duration.ofSeconds(1), CoreModuleProperties.HEARTBEAT_INTERVAL.getRequired(client))
        assertEquals(2, CoreModuleProperties.HEARTBEAT_NO_REPLY_MAX.getRequired(client))
        assertTrue(CoreModuleProperties.HEARTBEAT_REQUEST.getRequired(client).isNotBlank())
    }

    @Test(expected = Exception::class)
    fun `wrong password is rejected, not silently accepted`() {
        BastionSshSession(
            host = "127.0.0.1",
            port = port,
            user = "tester",
            knownHostsFile = knownHostsFile,
        ).use { session ->
            session.connect(password = "fel-lösenord", timeoutSeconds = 5)
        }
    }

    private fun createServer(requestedPort: Int): SshServer = SshServer.setUpDefaultServer().also { sshd ->
        sshd.port = requestedPort
        sshd.keyPairProvider = SimpleGeneratorHostKeyProvider(
            Files.createTempFile("bastion-test-hostkey", ".ser")
        )
        sshd.passwordAuthenticator = PasswordAuthenticator { username, password, _ ->
            username == "tester" && password == "s3cret"
        }
        sshd.commandFactory = CommandFactory { _, command -> EchoCommand(command) }
        sshd.start()
    }
}

/** Testserverns "echo"-kommando — körs bara "echo <resten av kommandot>". */
private class EchoCommand(private val commandLine: String) : org.apache.sshd.server.command.Command {
    private lateinit var out: OutputStream
    private lateinit var exitCallback: org.apache.sshd.server.ExitCallback

    override fun setInputStream(input: java.io.InputStream) {}
    override fun setOutputStream(out: OutputStream) { this.out = out }
    override fun setErrorStream(err: OutputStream) {}
    override fun setExitCallback(callback: org.apache.sshd.server.ExitCallback) { this.exitCallback = callback }

    override fun start(channel: org.apache.sshd.server.channel.ChannelSession, env: org.apache.sshd.server.Environment) {
        val text = commandLine.removePrefix("echo ").trim()
        out.write((text + "\n").toByteArray(Charsets.UTF_8))
        out.flush()
        exitCallback.onExit(0)
    }

    override fun destroy(channel: org.apache.sshd.server.channel.ChannelSession) {}
}
