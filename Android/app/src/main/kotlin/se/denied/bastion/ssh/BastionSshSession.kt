package se.denied.bastion.ssh

// Beteendeneutral CodeQL-trigger: håll Kotlin i PR-diffen så default setup producerar java-kotlin-konfigurationen som main-rulesetet kräver.

import org.apache.sshd.client.SshClient
import org.apache.sshd.client.channel.ClientChannelEvent
import org.apache.sshd.client.keyverifier.AcceptAllServerKeyVerifier
import org.apache.sshd.client.keyverifier.KnownHostsServerKeyVerifier
import org.apache.sshd.client.session.ClientSession
import org.apache.sshd.core.CoreModuleProperties
import java.io.ByteArrayOutputStream
import java.net.SocketAddress
import java.nio.file.Path
import java.security.PublicKey
import java.time.Duration
import java.util.Collection
import java.util.EnumSet
import java.util.concurrent.TimeUnit
import java.util.function.Supplier

/**
 * Minsta gemensamma SSH-kärna på Android-sidan, motsvarande SSHSession.swift
 * (SSHCore) — men bara det som verkligen behövs för att bevisa att en
 * anslutning fungerar: connect/run/close på lösenordsautentisering. Jump
 * hosts, streaming exec och nyckelbaserad auth är UTELÄMNADE tills det finns
 * en verklig UI att koppla dem till, inte gissat i förväg.
 *
 * Servernycklar verifieras med persistent TOFU mot [knownHostsFile]. En okänd
 * värd accepteras första gången och skrivs till filen; en senare ändrad nyckel
 * för samma värd avvisas. Läs-, parse- och skrivfel i known_hosts avvisar också
 * anslutningen i stället för att degradera till osäker verifiering.
 *
 * En autentiserad session skickar svarsbärande SSH-heartbeats. Om servern
 * slutar svara stänger Apache MINA SSHD sessionen efter det konfigurerade
 * antalet obesvarade heartbeats i stället för att lämna en tyst död session.
 */
class BastionSshSession(
    private val host: String,
    private val port: Int,
    private val user: String,
    knownHostsFile: Path,
    heartbeatIntervalSeconds: Long = DEFAULT_HEARTBEAT_INTERVAL_SECONDS,
    heartbeatMaxNoReply: Int = DEFAULT_HEARTBEAT_MAX_NO_REPLY,
) : AutoCloseable {

    private val client: SshClient = SshClient.setUpDefaultClient().also {
        it.serverKeyVerifier = FailClosedKnownHostsServerKeyVerifier(knownHostsFile)
        configureHeartbeat(it, heartbeatIntervalSeconds, heartbeatMaxNoReply)
    }
    private var session: ClientSession? = null

    fun connect(password: String, timeoutSeconds: Long = 10) {
        client.start()
        val s = client.connect(user, host, port)
            .verify(timeoutSeconds, TimeUnit.SECONDS)
            .session
        s.addPasswordIdentity(password)
        try {
            s.auth().verify(timeoutSeconds, TimeUnit.SECONDS)
        } catch (e: Exception) {
            // Misslyckad auth stänger INTE sessionen automatiskt (MINA SSHD
            // betraktar auth som ett separat steg från själva transporten) —
            // utan den här closen läcker den öppna sessionen, eftersom `s`
            // aldrig tilldelas `session` och close() därför inte når den.
            s.close(false)
            throw e
        }
        session = s
    }

    fun run(command: String, timeoutSeconds: Long = 10): String {
        val s = checkNotNull(session) { "connect() måste anropas innan run()" }
        val out = ByteArrayOutputStream()
        s.createExecChannel(command).use { channel ->
            channel.out = out
            channel.open().verify(timeoutSeconds, TimeUnit.SECONDS)
            val events = channel.waitFor(
                EnumSet.of(ClientChannelEvent.CLOSED),
                TimeUnit.SECONDS.toMillis(timeoutSeconds),
            )
            check(!events.contains(ClientChannelEvent.TIMEOUT)) {
                "Kommandot svarade inte inom ${timeoutSeconds}s: $command"
            }
        }
        // ByteArrayOutputStream.toString(Charset) kräver API 33 (minSdk är
        // 26 — verifierat i CI: "Call requires API level 33"). Kotlins
        // String(bytes, charset)-konstruktor gör exakt samma sak men är
        // ren Kotlin stdlib, ingen java.io-nivåbegränsning.
        return String(out.toByteArray(), Charsets.UTF_8)
    }

    override fun close() {
        session?.close(false)
        client.stop()
    }

    internal companion object {
        const val DEFAULT_HEARTBEAT_INTERVAL_SECONDS = 15L
        const val DEFAULT_HEARTBEAT_MAX_NO_REPLY = 3

        fun configureHeartbeat(client: SshClient, intervalSeconds: Long, maxNoReply: Int) {
            require(intervalSeconds > 0) { "heartbeatIntervalSeconds måste vara > 0" }
            require(maxNoReply > 0) { "heartbeatMaxNoReply måste vara > 0" }
            CoreModuleProperties.HEARTBEAT_INTERVAL.set(client, Duration.ofSeconds(intervalSeconds))
            CoreModuleProperties.HEARTBEAT_NO_REPLY_MAX.set(client, maxNoReply)
        }
    }
}

/**
 * MINA:s standardimplementation accepterar i vissa felvägar en okänd nyckel
 * även när known_hosts inte kan läsas eller uppdateras. Bastion ska i stället
 * fail-closed: TOFU är bara giltigt om den observerade nyckeln kan persisteras
 * och senare läsas tillbaka.
 */
private class FailClosedKnownHostsServerKeyVerifier(
    knownHostsFile: Path,
) : KnownHostsServerKeyVerifier(AcceptAllServerKeyVerifier.INSTANCE, knownHostsFile) {

    override fun acceptIncompleteHostKeys(
        clientSession: ClientSession,
        remoteAddress: SocketAddress,
        serverKey: PublicKey,
        reason: Throwable,
    ): Boolean = false

    override fun getKnownHostSupplier(
        clientSession: ClientSession?,
        file: Path,
    ): Supplier<Collection<HostEntryPair>> = Supplier {
        reloadKnownHosts(clientSession, file)
    }

    override fun handleKnownHostsFileUpdateFailure(
        clientSession: ClientSession,
        remoteAddress: SocketAddress,
        serverKey: PublicKey,
        file: Path,
        knownHosts: Collection<HostEntryPair>,
        reason: Throwable,
    ) {
        throw IllegalStateException("known_hosts kunde inte uppdateras: $file", reason)
    }
}
