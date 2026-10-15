# Protocol test fixtures

Small USB and receiver report descriptors, LED settings, key matrices and read
responses used by the automated tests. These fixtures contain no typing events,
macro sequences, serial numbers or personal profiles.

The wired matrix has 512 bytes; the receiver exposes its first 504 bytes. Radio
reports have 20 bytes each. The LED payload has 128 bytes on either connection.
