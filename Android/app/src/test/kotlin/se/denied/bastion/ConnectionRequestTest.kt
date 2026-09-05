package se.denied.bastion

import org.junit.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class ConnectionRequestTest {
    @Test
    fun parsesAndNormalizesConnectionInput() {
        val request = ConnectionRequest.parse(
            host = " example.com ",
            port = " 2222 ",
            user = " admin ",
            password = " secret ",
            command = " uptime ",
        ).getOrThrow()

        assertEquals("example.com", request.host)
        assertEquals(2222, request.port)
        assertEquals("admin", request.user)
        assertEquals(" secret ", request.password)
        assertEquals("uptime", request.command)
    }

    @Test
    fun parsesCredentialsWithoutRequiringAnExecCommand() {
        val credentials = ConnectionCredentials.parse(
            host = " terminal.example.com ",
            port = " 22 ",
            user = " shell-user ",
            password = " secret ",
        ).getOrThrow()

        assertEquals("terminal.example.com", credentials.host)
        assertEquals(22, credentials.port)
        assertEquals("shell-user", credentials.user)
        assertEquals(" secret ", credentials.password)
    }

    @Test
    fun rejectsMissingRequiredFieldsAndInvalidPorts() {
        assertTrue(ConnectionRequest.parse("", "22", "admin", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "0", "admin", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "65536", "admin", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "22", "", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "22", "admin", "", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "22", "admin", "secret", " ").isFailure)

        assertTrue(ConnectionCredentials.parse("", "22", "admin", "secret").isFailure)
        assertTrue(ConnectionCredentials.parse("host", "0", "admin", "secret").isFailure)
        assertTrue(ConnectionCredentials.parse("host", "22", "", "secret").isFailure)
        assertTrue(ConnectionCredentials.parse("host", "22", "admin", "").isFailure)
    }
}
