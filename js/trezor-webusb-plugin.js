// type TrezorDeviceInfoDebug = {path: string, debug: boolean};

const T1HID_VENDOR = 0x534c;
const TREZOR_DESCS = [
    // TREZOR v1
    // won't get opened, but we can show error at least
    {vendorId: 0x534c, productId: 0x0001},
    // TREZOR webusb Bootloader
    {vendorId: 0x1209, productId: 0x53c0},
    // TREZOR webusb Firmware
    {vendorId: 0x1209, productId: 0x53c1},
];

const CONFIGURATION_ID = 1;
const INTERFACE_ID = 0;
const ENDPOINT_ID = 1;
const DEBUG_INTERFACE_ID = 1;
const DEBUG_ENDPOINT_ID = 2;

export class TrezorWebUsbPlugin {
    // boolean
    unreadableHidDevice = false;

    // number
    configurationId = CONFIGURATION_ID;
    // number
    normalInterfaceId = INTERFACE_ID;
    // number
    normalEndpointId = ENDPOINT_ID;
    // number
    debugInterfaceId = DEBUG_INTERFACE_ID;
    // number
    debugEndpointId = DEBUG_ENDPOINT_ID;

    // @debug - boolean
    constructor() {
        const usb = navigator.usb;
        if (usb == null) {
            throw new Error(`WebUSB is not available on this browser.`);
        }
        this.usb = usb;
    }

    init() {
        return TrezorWebUsbPlugin();
    }

    // @device - USBDevice
    // @return - boolean
    _deviceHasDebugLink(device) {
        try {
            const iface = device.configurations[0].interfaces[DEBUG_INTERFACE_ID].alternates[0];
            return iface.interfaceClass === 255 && iface.endpoints[0].endpointNumber === DEBUG_ENDPOINT_ID;
        } catch (e) {
            console.error(`Error getting interface: ${e}`);
            return false;
        }
    }

    // @return - boolean
    _deviceIsHid(device) {
        return device.vendorId === T1HID_VENDOR;
    }

    // @return - Promise<Array<{path: string, device: USBDevice, debug: boolean}>>
    async _listDevices() {
        let bootloaderId = 0;
        const devices = await this.usb.getDevices();
        const trezorDevices = devices.filter(actualDevice => {
            console.info(`Found device: vendorId=${actualDevice.vendorId} productId=${actualDevice.productId} serial=${actualDevice.serialNumber}`);
            const isTrezor = TREZOR_DESCS.some(expectedDevice =>
                actualDevice.vendorId === expectedDevice.vendorId && actualDevice.productId === expectedDevice.productId
            );
            return isTrezor;
        });
        const hidDevices = trezorDevices.filter(dev => this._deviceIsHid(dev));
        const nonHidDevices = trezorDevices.filter(dev => !this._deviceIsHid(dev));

        console.log(`hidDevices: ${hidDevices.length}`);
        console.log(`nonHidDevices: ${nonHidDevices.length}`);

        this._lastDevices = nonHidDevices.map(device => {
            // path is just serial number
            // more bootloaders => number them, hope for the best
            const serialNumber = device.serialNumber;
            let path = (serialNumber == null || serialNumber === ``) ? `bootloader` : serialNumber;
            if (path === `bootloader`) {
                bootloaderId++;
                path = path + bootloaderId;
            }
            const debug = this._deviceHasDebugLink(device);
            console.log(`Device has been confirmed: path=${path} debug=${debug}`);
            return {path, device, debug};
        });

        const oldUnreadableHidDevice = this.unreadableHidDevice;
        this.unreadableHidDevice = hidDevices.length > 0;

        if (oldUnreadableHidDevice !== this.unreadableHidDevice) {
            // TODO
        }

        return this._lastDevices;
    }

    // Array<{path: string, device: USBDevice, debug: boolean}>
    _lastDevices = [];


    // @return - Promise<Array<TrezorDeviceInfoDebug>>
    async enumerate() {
        return (await this._listDevices()).map(info => ({path: info.path, debug: info.debug}));
    }

    _findDeviceInfo(path) {
        const deviceO = (this._lastDevices).find(d => d.path === path);
        if (deviceO == null) {
            throw new Error(`No such device: ${path}.`);
        }
        return deviceO;
    }


    // @path - string
    // @data - ArrayBuffer
    // @debug - boolean
    // @return - Promise<void>
    async send(path, data) {
        // USBDevice
        const {device, debug} = this._findDeviceInfo(path);
        const endpoint = this._endpointId(debug);

        // const newArray = new Uint8Array(64);
        // newArray[0] = 63;
        // newArray.set(new Uint8Array(data), 1);

        if (!device.opened) {
            await this.connect(path, debug, false);
        }

        return device.transferOut(endpoint, data).then(() => {
        });
    }

    // @path - string
    // @debug - boolean
    // @return - Promise<ArrayBuffer>
    async receive(path) {
        const {device, debug} = this._findDeviceInfo(path);
        const endpoint = this._endpointId(debug);

        try {
            if (!device.opened) {
                await this.connect(path, debug, false);
            }

            const res = await device.transferIn(endpoint, 64);
            if (res.data.byteLength === 0) {
                return this.receive(path, debug);
            }
            return res.data.buffer;
            // return res.data.buffer.slice(1);
        } catch (e) {
            if (e.message === `Device unavailable.`) {
                throw new Error(`Action was interrupted due to the device being unavailable.`);
            } else {
                throw e;
            }
        }
    }

    // @path - string
    // @debug - boolean
    // @first - boolean
    // @return - Promise<void>
    async connect(path, first) {
        console.log(`Connect to ${path}: first=${first}`);
        for (let i = 0; i < 5; i++) {
            if (i > 0) {
                await new Promise((resolve) => setTimeout(() => resolve(), i * 200));
            }
            try {
                return await this._connectIn(path, true);
            } catch (e) {
                // ignore
                if (i === 4) {
                    throw e;
                }
            }
        }
    }

    // @debug - boolean
    _endpointId(debug) {
        return debug ? this.debugEndpointId : this.normalEndpointId;
    }

    // @debug - boolean
    _interfaceId(debug) {
        return debug ? this.debugInterfaceId : this.normalInterfaceId;
    }


    // @path - string
    // @debug - boolean
    // @first - boolean
    // @return - Promise<void>
    async _connectIn(path, first) {
        console.log(`TrezorWebUsbPlugin._connectIn() ${path}: first=${first}`);
        const {device, debug} = this._findDeviceInfo(path);
        await device.open();

        if (first) {
            await device.selectConfiguration(this.configurationId);
            try {
                // reset fails on ChromeOS and windows
                await device.reset();
            } catch (error) {
                // do nothing
            }
        }

        const interfaceId = this._interfaceId(debug);
        console.log(`TrezorWebUsbPlugin._connectIn() interface=${interfaceId} debug=${debug}`);
        await device.claimInterface(interfaceId);
    }

    // @path - string
    // @debug - boolean
    // @last - boolean
    // @return - Promise<void>
    async disconnect(path, last) {
        const {device, debug} = this._findDeviceInfo(path);

        const interfaceId = this._interfaceId(debug);
        await device.releaseInterface(interfaceId);
        if (last) {
            await device.close();
        }
    }

    // @return - Promise<void>
    async requestDevice() {
        // I am throwing away the resulting device, since it appears in enumeration anyway
        await this.usb.requestDevice({filters: TREZOR_DESCS});
    }
}
