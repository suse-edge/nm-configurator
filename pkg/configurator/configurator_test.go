package configurator

import (
	"os"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/suse-edge/nm-configurator/pkg/config"
)

func TestConfigurator_Run(t *testing.T) {
	const destDir = "testdata/out"
	require.Nil(t, os.MkdirAll(destDir, 0755))

	defer func() {
		assert.Nil(t, os.RemoveAll(destDir))
	}()

	tests := []struct {
		name            string
		conf            *config.Config
		localInterfaces NetworkInterfaces
		expectedErr     string
	}{
		{
			name: "configurator fails due to none of the preconfigured hosts matching local interfaces",
			conf: &config.Config{
				Hosts: []*config.Host{
					{
						Name: "host1",
						Interfaces: []*config.Interface{
							{
								LogicalName: "eth0",
								MACAddress:  "00:11:22:33:44:55",
							},
						},
					},
					{
						Name: "host2",
						Interfaces: []*config.Interface{
							{
								LogicalName: "eth0",
								MACAddress:  "00:11:22:33:44:56",
							},
						},
					},
				},
			},
			localInterfaces: map[string]string{
				"00:10:20:30:40:50": "eth0",
			},
			expectedErr: "identifying host: none of the preconfigured hosts match local NICs",
		},
		{
			name: "configurator fails due to reading from non-existing config dir",
			conf: &config.Config{
				SourceDir: "some-non-existing-dir-123",
				Hosts: []*config.Host{
					{
						Name: "host1",
						Interfaces: []*config.Interface{
							{
								LogicalName: "eth0",
								MACAddress:  "00:11:22:33:44:55",
							},
						},
					},
				},
			},
			localInterfaces: map[string]string{
				"00:11:22:33:44:55": "eth0",
			},
			expectedErr: "copying files: open some-non-existing-dir-123/host1: no such file or directory",
		},
		{
			name: "configurator fails due to parsing invalid config file",
			conf: &config.Config{
				SourceDir: "testdata",
				Hosts: []*config.Host{
					{
						Name: "host2",
						Interfaces: []*config.Interface{
							{
								LogicalName: "eth0",
								MACAddress:  "00:11:22:33:44:57",
							},
						},
					},
				},
			},
			localInterfaces: map[string]string{
				"00:11:22:33:44:57": "eth0",
			},
			expectedErr: "copying files: loading file \"testdata/host2/invalid.nmconnection\": key-value delimiter not found: -[connection]\n",
		},
		{
			name: "configurator fails due to storing to non-existing destination dir",
			conf: &config.Config{
				SourceDir:      "testdata",
				DestinationDir: "some-non-existing-dir-123",
				Hosts: []*config.Host{
					{
						Name: "host1",
						Interfaces: []*config.Interface{
							{
								LogicalName: "eth0",
								MACAddress:  "00:11:22:33:44:57",
							},
						},
					},
				},
			},
			localInterfaces: map[string]string{
				"00:11:22:33:44:57": "eth0",
			},
			expectedErr: "copying files: open some-non-existing-dir-123/eth0.nmconnection: no such file or directory",
		},
		{
			name: "configurator executed successfully",
			conf: &config.Config{
				SourceDir:      "testdata",
				DestinationDir: destDir,
				Hosts: []*config.Host{
					{
						Name: "host1",
						Interfaces: []*config.Interface{
							{
								LogicalName: "eth0",
								MACAddress:  "00:11:22:33:44:55",
							},
						},
					},
				},
			},
			localInterfaces: map[string]string{
				"00:11:22:33:44:55": "eth1",
			},
			expectedErr: "",
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			configurator := New(test.conf, test.localInterfaces)

			err := configurator.Run()

			if test.expectedErr == "" {
				assert.Nil(t, err)
			} else {
				assert.EqualError(t, err, test.expectedErr)
			}
		})
	}
}
