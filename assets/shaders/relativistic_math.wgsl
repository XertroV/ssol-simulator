// Ported from relativity.shader
// Source: OpenRelativity; author: MITGameLab and contributors; license: MIT License

const xla = 0.39952807612909519;
const xlb = 444.63156780935032;
const xlc = 20.095464678736523;

const xha = 1.1305579611401821;
const xhb = 593.23109262398259;
const xhc = 34.446036241271742;

const ya = 1.0098874822455657;
const yb = 556.03724875218927;
const yc = 46.184868454550838;

const za = 2.0648400466720593;
const zb = 448.45126344558236;
const zc = 22.357297606503543;

const IR_RANGE: f32 = 400.0;
const IR_START: f32 = 700.0;
const UV_RANGE: f32 = 380.0;
const UV_START: f32 = 0.0;

const PI = 3.14159265358979323;

fn RGBToXYZC(r: f32, g: f32, b: f32) -> vec3<f32> {
    var xyz: vec3<f32>;
    xyz.x = 0.13514 * r + 0.120432 * g + 0.057128 * b;
    xyz.y = 0.0668999 * r + 0.232706 * g + 0.0293946 * b;
    xyz.z = 0.0 * r + 0.0000218959 * g + 0.358278 * b;
    return xyz;
}

fn XYZToRGBC(x: f32, y: f32, z: f32) -> vec3<f32> {
    var rgb: vec3<f32>;
    rgb.x = 9.94845 * x - 5.1485 * y - 1.16389 * z;
    rgb.y = -2.86007 * x + 5.77745 * y - 0.0179627 * z;
    rgb.z = 0.000174791 * x - 0.000353084 * y + 2.79113 * z;
    return rgb;
}

fn weightFromXYZCurves(xyz: vec3<f32>) -> vec3<f32> {
    var returnVal: vec3<f32>;
    returnVal.x = 0.0735806 * xyz.x - 0.0380793 * xyz.y - 0.00860837 * xyz.z;
    returnVal.y = -0.0665378 * xyz.x + 0.134408 * xyz.y - 0.000417865 * xyz.z;
    returnVal.z = 0.00000299624 * xyz.x - 0.00000605249 * xyz.y + 0.0484424 * xyz.z;
    return returnVal;
}

fn getXFromCurve(param: vec3<f32>, shift: f32) -> f32 {
    let top1 = param.x * xla * exp(-(pow((param.y * shift) - xlb, 2.0) / (2.0 * (pow(param.z * shift, 2.0) + pow(xlc, 2.0))))) * sqrt(2.0 * PI);
    let bottom1 = sqrt((1.0 / pow(param.z * shift, 2.0)) + (1.0 / pow(xlc, 2.0)));

    let top2 = param.x * xha * exp(-(pow((param.y * shift) - xhb, 2.0) / (2.0 * (pow(param.z * shift, 2.0) + pow(xhc, 2.0))))) * sqrt(2.0 * PI);
    let bottom2 = sqrt((1.0 / pow(param.z * shift, 2.0)) + (1.0 / pow(xhc, 2.0)));

    return (top1 / bottom1) + (top2 / bottom2);
}

fn getYFromCurve(param: vec3<f32>, shift: f32) -> f32 {
    let top = param.x * ya * exp(-(pow((param.y * shift) - yb, 2.0) / (2.0 * (pow(param.z * shift, 2.0) + pow(yc, 2.0))))) * sqrt(2.0 * PI);
    let bottom = sqrt((1.0 / pow(param.z * shift, 2.0)) + (1.0 / pow(yc, 2.0)));
    return top / bottom;
}

fn getZFromCurve(param: vec3<f32>, shift: f32) -> f32 {
    let top = param.x * za * exp(-(pow((param.y * shift) - zb, 2.0) / (2.0 * (pow(param.z * shift, 2.0) + pow(zc, 2.0))))) * sqrt(2.0 * PI);
    let bottom = sqrt((1.0 / pow(param.z * shift, 2.0)) + (1.0 / pow(zc, 2.0)));
    return top / bottom;
}

fn constrainRGB(r: f32, g: f32, b: f32) -> vec3<f32> {
    var w: f32;
    w = min(0.0, min(r, min(g, b)));
    w = -w;

    var res = vec3(r, g, b);
    if (w > 0.0) {
        res += w;
    }
    w = max(res.x, max(res.y, res.z));

    if (w > 1.0) {
        res /= w;
    }
    return res;
}
