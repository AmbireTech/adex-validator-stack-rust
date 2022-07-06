const request = require('request');

const valid_campaign = {
    id: null,
    channel: {
        leader: "0x80690751969B234697e9059e04ed72195c3507fa",
        follower: "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
        guardian: "0xe061E1EB461EaBE512759aa18A201B20Fe90631D",
        token: "0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E",
        nonce: 987654321,
    },
    creator:"0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F",
    budget:100000000000,
    validators: [
        {
            id:"0x80690751969B234697e9059e04ed72195c3507fa",
            fee:2000000,
            url:"http://localhost:8005"
        },
        {
            id:"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
            fee:3000000,
            url:"http://localhost:8006"
        }
    ],
    title:"Dummy Campaign",
    pricingBounds:{
        CLICK: {min: 0,max:0 },
        IMPRESSION:{min:1,max:10}},
    eventSubmission:{allow:[]},
    targetingRules:[],
    created:1612162800000,
    active_to:4073414400000
}
const chain = {
    chain_id: 1,
    rpc: "http://dummy.com",
    outpace: "0x0000000000000000000000000000000000000000"
};

// Valid Request
const auth = {
    era: 0,
    uid: "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F",
    chain,
};


request({
    url: "http://localhost:8005/v5/campaign",
    method: "POST",
    json: true,   // <--Very important!!!
    body: JSON.stringify(valid_campaign)
}, (err, res, body) => {
    console.log(res); // should be a Campaign object
});

// Request with a broken campaign


// Request from a different creator