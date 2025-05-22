# Data structure definitions

This file contains a non exhaustive list of descriptions for different data structures used for communication.

## Authorization token
The authorization token is used to provide all necessary information + authorization for a device to be added to a users account. It consists of
 - A token - used for authorizing the device
 - the users uuid - used for identification of the user
 - the users public key - used to encrypt the data stored on the server
 - the users email - used to display in the configured users section
 - the checksum is a argon2id hash since we cant use the browsers implementation of sha-256 in the webinterface of a device. It is computed with a empty salt with the size of 8, 19 MB Memory, time 2 and parallelism 1 parameters.

The token is base58 encoded using ther FLICKR-alphabet and structured as followed:

<table>
    <tr>
        <th>
            Name
        </th>
        <td>
            Authorization
        </td>
        <td>
            User uuid
        </td>
        <td>
            User public key
        </td>
        <td>
            User email
        </td>
        <td>
            Checksum
        </td>
    </tr>
    <tr>
        <th>
            Size
        </th>
        <td>
            32 Bytes
        </td>
        <td>
            36 Bytes
        </td>
        <td>
            32 Bytes
        </td>
        <td>
            Variable
        </td>
        <td>
            32 Bytes
        </td>
    </tr>
</table>
