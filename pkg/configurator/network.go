package configurator

import (
	"net"
	"strings"
)

// NetworkInterfaces maps system network interfaces.
//
// Key is MAC Address.
// Value is Name.
type NetworkInterfaces map[string]string

func GetNetworkInterfaces() (NetworkInterfaces, error) {
	interfaces, err := net.Interfaces()
	if err != nil {
		return nil, err
	}

	interfaceAddresses := map[string]string{}

	for _, i := range interfaces {
		if i.HardwareAddr == nil {
			// omit loopback / virtual interfaces
			continue
		}

		address := strings.ToLower(i.HardwareAddr.String())
		interfaceAddresses[address] = i.Name
	}

	return interfaceAddresses, nil
}
