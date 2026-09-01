package se.denied.bastion.ssh

// Beteendeneutral CodeQL-bootstrap: håll Kotlin i migrations-PR-diffen tills advanced setup är aktivt och producerar java-kotlin-konfigurationen på varje PR.

import org.apache.sshd.client.SshClient
import org.apache.sshd.client.session.ClientSession
import org.apache.sshd.client.channel.ClientChannelEvent
import java.io.ByteArrayOutputStream
import java.util.EnumSet
import java.util.concurrent.TimeUnit

/**
 * Minsta gemensamma SSH-kärna på Android-sidan, motsvarande SSHSession.swift
 * (SSHCore) — men bara det som verkligen behövs för att bevisa att en
 * anslutning fungerar: connect/run/close på lösenordsautentisering. Jump
 * hosts, streaming exec och nyckelbaserad auth är UTELÄMNADE tills det finns
 * en verklig UI att koppla dem till, inte gissat i förväg.
 */
class BastionSshSession(
    private val host: String,
    private val port: Int,
    private val user: String,
) : AutoCloseable {

    private val client: SshClient = SshClient.setUpDefaultClient()
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
}
