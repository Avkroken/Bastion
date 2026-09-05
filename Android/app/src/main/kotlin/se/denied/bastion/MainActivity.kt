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
            text = "Fyll i anslutningsuppgifterna. Inga uppgifter sparas."
            setTextIsSelectable(true)
        }
        val connect = Button(this).apply { text = "Anslut och kör" }

        val content = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(dp(20), dp(20), dp(20), dp(20))
            addView(host)
            addView(port)
            addView(user)
            addView(password)
            addView(command)
            addView(connect)
            addView(status)
        }

        setContentView(ScrollView(this).apply { addView(content) })

        connect.setOnClickListener {
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

            connect.isEnabled = false
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
                    connect.isEnabled = true
                    status.text = result.fold(
                        onSuccess = { output -> output.ifBlank { "Kommandot slutfördes utan output." } },
                        onFailure = { error ->
                            "Anslutningen misslyckades: ${error.message ?: error.javaClass.simpleName}"
                        },
                    )
                }
            }.start()
        }
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
}
