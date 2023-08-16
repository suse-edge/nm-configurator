package configurator

import "net"

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

		interfaceAddresses[i.HardwareAddr.String()] = i.Name
	}

	return interfaceAddresses, nil
}
