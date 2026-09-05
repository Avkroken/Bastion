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
    private var port: Int = 0

    @Before
    fun startServer() {
        server = SshServer.setUpDefaultServer()
        server.port = 0 // slumpad ledig port
        server.keyPairProvider = SimpleGeneratorHostKeyProvider(
            Files.createTempFile("bastion-test-hostkey", ".ser")
        )
        server.passwordAuthenticator = PasswordAuthenticator { username, password, _ ->
            username == "tester" && password == "s3cret"
        }
        server.commandFactory = CommandFactory { _, command ->
            EchoCommand(command)
        }
        server.start()
        port = server.port
    }

    @After
    fun stopServer() {
        server.stop(true)
    }

    @Test
    fun `connect authenticate run and get real output back`() {
        BastionSshSession(host = "127.0.0.1", port = port, user = "tester").use { session ->
            session.connect(password = "s3cret")
            val output = session.run("echo hello-from-bastion")
            assertEquals("hello-from-bastion\n", output)
        }
    }

    @Test
    fun `response-bearing heartbeat keeps an idle authenticated session usable`() {
        BastionSshSession(
            host = "127.0.0.1",
            port = port,
            user = "tester",
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
        BastionSshSession(host = "127.0.0.1", port = port, user = "tester").use { session ->
            session.connect(password = "fel-lösenord", timeoutSeconds = 5)
        }
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
