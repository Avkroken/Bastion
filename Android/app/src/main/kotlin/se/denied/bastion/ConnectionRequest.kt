package se.denied.bastion

data class ConnectionCredentials(
    val host: String,
    val port: Int,
    val user: String,
    val password: String,
) {
    companion object {
        fun parse(
            host: String,
            port: String,
            user: String,
            password: String,
        ): Result<ConnectionCredentials> = runCatching {
            val cleanHost = host.trim()
            require(cleanHost.isNotEmpty()) { "Värd saknas" }

            val cleanUser = user.trim()
            require(cleanUser.isNotEmpty()) { "Användare saknas" }
            require(password.isNotEmpty()) { "Lösenord saknas" }

            val parsedPort = port.trim().toIntOrNull()
            require(parsedPort != null && parsedPort in 1..65535) { "Port måste vara 1–65535" }

            ConnectionCredentials(
                host = cleanHost,
                port = parsedPort,
                user = cleanUser,
                password = password,
            )
        }
    }
}

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
        ): Result<ConnectionRequest> = ConnectionCredentials.parse(
            host = host,
            port = port,
            user = user,
            password = password,
        ).mapCatching { credentials ->
            val cleanCommand = command.trim()
            require(cleanCommand.isNotEmpty()) { "Kommando saknas" }

            ConnectionRequest(
                host = credentials.host,
                port = credentials.port,
                user = credentials.user,
                password = credentials.password,
                command = cleanCommand,
            )
        }
    }
}
