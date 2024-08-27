
interface CircleParams {
    color: "red" | "green"
}

export function Circle(params: CircleParams) {
    return <div style={`height:20px;width:20px;background-color:${params.color};border-radius:50%;display:inline-block;`} />
    // return <div style={`width:16px;heigth:16px;background:${params.color};border-radius:50%;-moz-border-radius:50%;-webkit-border-radius:50%;line-height:16px;vertical-align:middle;text-align:center;color:white;`}>*</div>
}
