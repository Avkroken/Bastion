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
    fun rejectsMissingRequiredFieldsAndInvalidPorts() {
        assertTrue(ConnectionRequest.parse("", "22", "admin", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "0", "admin", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "65536", "admin", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "22", "", "secret", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "22", "admin", "", "uptime").isFailure)
        assertTrue(ConnectionRequest.parse("host", "22", "admin", "secret", " ").isFailure)
    }
}
