package se.denied.bastion

import android.app.Activity
import android.os.Bundle
import android.text.InputType
import android.view.ViewGroup
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import se.denied.bastion.ssh.BastionSshSession
import java.io.File

class MainActivity : Activity() {
    private var terminalSession: BastionSshSession? = null
    private var terminalShell: BastionSshSession.InteractiveShell? = null
    private val terminalOutputLock = Any()
    private val pendingTerminalOutput = StringBuilder()
    private var terminalOutputDrainScheduled = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val host = field("Värd")
        val port = field("Port", InputType.TYPE_CLASS_NUMBER).apply { setText("22") }
        val user = field("Användare")
        val password = field(
            "Lösenord",
            InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD,
        )
        val command = field("Kommando").apply { setText("uname -a") }
        val status = TextView(this).apply {
            text = "Inloggningsuppgifter sparas inte. Värdens adress och publika host key sparas lokalt i known_hosts."
            setTextIsSelectable(true)
        }
        val runCommand = Button(this).apply { text = "Anslut och kör" }

        val terminalOutput = TextView(this).apply {
            text = "Terminalen är frånkopplad."
            setTextIsSelectable(true)
        }
        val terminalInput = field("Terminalinmatning").apply {
            isEnabled = false
            setSingleLine(true)
        }
        val openTerminal = Button(this).apply { text = "Öppna terminal" }
        val sendTerminal = Button(this).apply {
            text = "Skicka rad"
            isEnabled = false
        }
        val disconnectTerminal = Button(this).apply {
            text = "Koppla från terminal"
            isEnabled = false
        }

        fun handleTerminalClosed(session: BastionSshSession, error: Throwable?) {
            runOnUiThread {
                if (terminalSession !== session) return@runOnUiThread
                terminalSession = null
                terminalShell = null
                terminalInput.isEnabled = false
                sendTerminal.isEnabled = false
                disconnectTerminal.isEnabled = false
                openTerminal.isEnabled = true
                val reason = error?.message?.let { ": $it" }.orEmpty()
                enqueueTerminalOutput(terminalOutput, "\nTerminalanslutningen stängdes$reason\n")
                closeTerminalResourcesAsync(null, session)
            }
        }

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(20), dp(20), dp(20))
            addView(host)
            addView(port)
            addView(user)
            addView(password)
            addView(command)
            addView(runCommand)
            addView(status)
            addView(openTerminal)
            addView(terminalOutput)
            addView(terminalInput)
            addView(sendTerminal)
            addView(disconnectTerminal)
        }

        setContentView(ScrollView(this).apply { addView(content) })

        runCommand.setOnClickListener {
            val request = ConnectionRequest.parse(
                host = host.text.toString(),
                port = port.text.toString(),
                user = user.text.toString(),
                password = password.text.toString(),
                command = command.text.toString(),
            ).getOrElse { error ->
                status.text = error.message ?: "Ogiltiga anslutningsuppgifter"
                return@setOnClickListener
            }

            runCommand.isEnabled = false
            status.text = "Ansluter…"

            Thread {
                val result = runCatching {
                    BastionSshSession(
                        host = request.host,
                        port = request.port,
                        user = request.user,
                        knownHostsFile = File(filesDir, "known_hosts").toPath(),
                    ).use { session ->
                        session.connect(request.password)
                        session.run(request.command)
                    }
                }

                runOnUiThread {
                    runCommand.isEnabled = true
                    status.text = result.fold(
                        onSuccess = { output -> output.ifBlank { "Kommandot slutfördes utan output." } },
                        onFailure = { error ->
                            "Anslutningen misslyckades: ${error.message ?: error.javaClass.simpleName}"
                        },
                    )
                }
            }.start()
        }

        openTerminal.setOnClickListener {
            val credentials = ConnectionCredentials.parse(
                host = host.text.toString(),
                port = port.text.toString(),
                user = user.text.toString(),
                password = password.text.toString(),
            ).getOrElse { error ->
                terminalOutput.text = error.message ?: "Ogiltiga anslutningsuppgifter"
                return@setOnClickListener
            }

            openTerminal.isEnabled = false
            terminalOutput.text = "Ansluter terminal…"

            Thread {
                val session = BastionSshSession(
                    host = credentials.host,
                    port = credentials.port,
                    user = credentials.user,
                    knownHostsFile = File(filesDir, "known_hosts").toPath(),
                )
                val result = runCatching {
                    session.connect(credentials.password)
                    session.openShell(
                        onClosed = { error -> handleTerminalClosed(session, error) },
                        onOutput = { chunk -> enqueueTerminalOutput(terminalOutput, chunk) },
                    )
                }
                if (result.isFailure) session.close()

                runOnUiThread {
                    result.fold(
                        onSuccess = { shell ->
                            if (isDestroyed || !shell.isOpen) {
                                closeTerminalResourcesAsync(shell, session)
                                if (!isDestroyed) {
                                    openTerminal.isEnabled = true
                                    terminalOutput.text = "Terminalanslutningen stängdes innan den blev klar."
                                }
                                return@runOnUiThread
                            }
                            terminalSession = session
                            terminalShell = shell
                            terminalOutput.text = "Ansluten. Skriv en rad och tryck Skicka rad.\n"
                            terminalInput.isEnabled = true
                            sendTerminal.isEnabled = true
                            disconnectTerminal.isEnabled = true
                        },
                        onFailure = { error ->
                            openTerminal.isEnabled = true
                            terminalOutput.text =
                                "Terminalanslutningen misslyckades: ${error.message ?: error.javaClass.simpleName}"
                        },
                    )
                }
            }.start()
        }

        sendTerminal.setOnClickListener {
            val shell = terminalShell ?: return@setOnClickListener
            val line = terminalInput.text.toString()
            terminalInput.text.clear()
            sendTerminal.isEnabled = false

            Thread {
                val result = runCatching { shell.sendLine(line) }
                runOnUiThread {
                    if (result.isFailure) {
                        enqueueTerminalOutput(
                            terminalOutput,
                            "\nInmatningen misslyckades: ${result.exceptionOrNull()?.message ?: "okänt fel"}\n",
                        )
                    }
                    sendTerminal.isEnabled = terminalShell != null
                }
            }.start()
        }

        disconnectTerminal.setOnClickListener {
            val shell = terminalShell
            val session = terminalSession
            terminalShell = null
            terminalSession = null
            terminalInput.isEnabled = false
            sendTerminal.isEnabled = false
            disconnectTerminal.isEnabled = false
            openTerminal.isEnabled = true
            enqueueTerminalOutput(terminalOutput, "\nFrånkopplad.\n")
            closeTerminalResourcesAsync(shell, session)
        }
    }

    override fun onDestroy() {
        val shell = terminalShell
        val session = terminalSession
        terminalShell = null
        terminalSession = null
        closeTerminalResourcesAsync(shell, session)
        super.onDestroy()
    }

    private fun enqueueTerminalOutput(view: TextView, chunk: String) {
        if (chunk.isEmpty()) return
        val shouldScheduleDrain = synchronized(terminalOutputLock) {
            pendingTerminalOutput.append(chunk)
            if (pendingTerminalOutput.length > MAX_TERMINAL_CHARS) {
                pendingTerminalOutput.delete(0, pendingTerminalOutput.length - MAX_TERMINAL_CHARS)
            }
            if (terminalOutputDrainScheduled) {
                false
            } else {
                terminalOutputDrainScheduled = true
                true
            }
        }
        if (!shouldScheduleDrain) return

        view.post {
            val pending = synchronized(terminalOutputLock) {
                val value = pendingTerminalOutput.toString()
                pendingTerminalOutput.setLength(0)
                terminalOutputDrainScheduled = false
                value
            }
            if (!isDestroyed && pending.isNotEmpty()) {
                appendTerminalOutput(view, pending)
            }
        }
    }

    private fun appendTerminalOutput(view: TextView, chunk: String) {
        if (chunk.length >= MAX_TERMINAL_CHARS) {
            view.text = chunk.takeLast(MAX_TERMINAL_CHARS)
            return
        }
        view.append(chunk)
        val overflow = view.text.length - MAX_TERMINAL_CHARS
        if (overflow > 0) {
            view.text = view.text.subSequence(overflow, view.text.length).toString()
        }
    }

    private fun closeTerminalResourcesAsync(
        shell: BastionSshSession.InteractiveShell?,
        session: BastionSshSession?,
    ) {
        if (shell == null && session == null) return
        Thread {
            shell?.close()
            session?.close()
        }.start()
    }

    private fun field(hint: String, inputType: Int = InputType.TYPE_CLASS_TEXT): EditText =
        EditText(this).apply {
            this.hint = hint
            this.inputType = inputType
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT,
            )
        }

    private fun dp(value: Int): Int = (value * resources.displayMetrics.density).toInt()

    private companion object {
        const val MAX_TERMINAL_CHARS = 200_000
    }
}
