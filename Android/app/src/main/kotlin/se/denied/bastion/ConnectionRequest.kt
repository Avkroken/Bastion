package se.denied.bastion

data class ConnectionRequest(
    val host: String,
    val port: Int,
    val user: String,
    val password: String,
    val command: String,
) {
    companion object {
        fun parse(
            host: String,
            port: String,
            user: String,
            password: String,
            command: String,
        ): Result<ConnectionRequest> = runCatching {
            val cleanHost = host.trim()
            require(cleanHost.isNotEmpty()) { "Värd saknas" }

            val cleanUser = user.trim()
            require(cleanUser.isNotEmpty()) { "Användare saknas" }
            require(password.isNotEmpty()) { "Lösenord saknas" }

            val parsedPort = port.trim().toIntOrNull()
            require(parsedPort != null && parsedPort in 1..65535) { "Port måste vara 1–65535" }

            val cleanCommand = command.trim()
            require(cleanCommand.isNotEmpty()) { "Kommando saknas" }

            ConnectionRequest(
                host = cleanHost,
                port = parsedPort,
                user = cleanUser,
                password = password,
                command = cleanCommand,
            )
        }
    }
}
